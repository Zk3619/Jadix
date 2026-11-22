import AppKit
import CoreImage
import CoreFoundation
import CoreGraphics
import CoreText
import CryptoKit
import Darwin
import ImageIO
import QuartzCore
import UniformTypeIdentifiers

private final class AppKitCallbackContext {
    weak var host: JadixNativeView?
}

private let measureAppKit: JadixNativeMeasure = { context, json, output in
    MainActor.assumeIsolated {
        guard let context, let json, let output,
              let host = Unmanaged<AppKitCallbackContext>.fromOpaque(context).takeUnretainedValue().host else {
            return false
        }
        do {
            let (size, baseline) = try host.measure(String(cString: json))
            output[0] = Double(size.width)
            output[1] = Double(size.height)
            output[2] = baseline.map { Double($0) } ?? .nan
            return true
        } catch { return false }
    }
}

private let publishAppKit: JadixNativePublish = { context, json in
    MainActor.assumeIsolated {
        guard let context, let json,
              let host = Unmanaged<AppKitCallbackContext>.fromOpaque(context).takeUnretainedValue().host else {
            return false
        }
        do { try host.publish(String(cString: json)); return true }
        catch { host.onError?(error); return false }
    }
}

private let clearAppKit: JadixNativeClear = { context in
    MainActor.assumeIsolated {
        guard let context else { return }
        Unmanaged<AppKitCallbackContext>.fromOpaque(context).takeUnretainedValue().host?.clear()
    }
}

private let requestAppKitFile: JadixNativeFileRequest = { context, request, json in
    MainActor.assumeIsolated {
        guard let context, let json,
              let host = Unmanaged<AppKitCallbackContext>.fromOpaque(context).takeUnretainedValue().host else {
            return false
        }
        return host.scheduleFileRequest(request, json: String(cString: json))
    }
}

private let requestAppKitWindow: JadixNativeWindowRequest = { context, request, json in
    MainActor.assumeIsolated {
        guard let context, let json,
              let host = Unmanaged<AppKitCallbackContext>.fromOpaque(context).takeUnretainedValue().host else {
            return false
        }
        return host.scheduleWindowRequest(request, json: String(cString: json))
    }
}

private let requestAppKitAlert: JadixNativeAlertRequest = { context, request, json in
    MainActor.assumeIsolated {
        guard let context, let json,
              let host = Unmanaged<AppKitCallbackContext>.fromOpaque(context).takeUnretainedValue().host else {
            return false
        }
        return host.scheduleAlertRequest(request, json: String(cString: json))
    }
}

private let requestAppKitShare: JadixNativeShareRequest = { context, request, json in
    MainActor.assumeIsolated {
        guard let context, let json,
              let host = Unmanaged<AppKitCallbackContext>.fromOpaque(context).takeUnretainedValue().host else {
            return false
        }
        return host.scheduleShareRequest(request, json: String(cString: json))
    }
}

private let requestAppKitContextMenu: JadixNativeContextMenuRequest = { context, request, json in
    MainActor.assumeIsolated {
        guard let context, let json,
              let host = Unmanaged<AppKitCallbackContext>.fromOpaque(context).takeUnretainedValue().host else {
            return false
        }
        return host.scheduleContextMenu(request, json: String(cString: json))
    }
}

private struct JadixContextMenuRequest: Decodable {
    struct Icon: Decodable {
        let source: String
        let template: Bool
    }

    struct Item: Decodable {
        enum Kind: String, Decodable { case command, submenu, separator }
        let kind: Kind
        let id: String?
        let label: String?
        let enabled: Bool?
        let checked: Bool?
        let role: String?
        let icon: Icon?
        let key: String?
        let modifiers: [String]?
        let items: [Item]?
    }

    let anchor: String
    let anchorRoot: String
    let items: [Item]
}

private struct JadixMenuBarRequest: Decodable {
    let revision: UInt64
    let windowMenu: Int?
    let items: [JadixContextMenuRequest.Item]
}

@MainActor
private final class JadixMenuTarget: NSObject, NSMenuItemValidation {
    var command: String?
    var dispatch: ((String) -> Void)?
    private let remoteImages: AppleRemoteImages
    private let iconPixels: Int
    private var iconLoads: [(url: URL, token: UUID)] = []
    private var iconsActive = true

    init(remoteImages: AppleRemoteImages, scale: CGFloat) {
        self.remoteImages = remoteImages
        iconPixels = Int((16 * scale).rounded(.up))
    }

    @objc func select(_ item: NSMenuItem) {
        guard let id = item.representedObject as? String else { return }
        if let dispatch { dispatch(id) }
        else { command = id }
    }

    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        menuItem.representedObject is String
    }

    func cancelIcons() {
        iconsActive = false
        for load in iconLoads { remoteImages.cancel(load.url, token: load.token) }
        iconLoads.removeAll()
    }

    private func loadIcon(_ icon: JadixContextMenuRequest.Icon?, into item: NSMenuItem) {
        guard let icon else { return }
        let request = AppleImageRequest(
            source: icon.source, pixelWidth: iconPixels, pixelHeight: iconPixels,
            fit: .contain, allowsAnimation: false
        )
        let transfer = remoteImages.decode(request) { [weak self, weak item] decoded in
            guard self?.iconsActive == true, let item, let frame = decoded?.frames.first else { return }
            let scale = 16 / CGFloat(max(frame.width, frame.height))
            let size = NSSize(width: CGFloat(frame.width) * scale, height: CGFloat(frame.height) * scale)
            let image = NSImage(cgImage: frame, size: size)
            image.isTemplate = icon.template
            item.image = image
            if item.responds(to: NSSelectorFromString("setPreferredImageVisibility:")) {
                item.setValue(1, forKey: "preferredImageVisibility")
            }
        }
        if let transfer { iconLoads.append(transfer) }
    }

    func build(_ items: [JadixContextMenuRequest.Item]) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = dispatch != nil
        for item in items {
            switch item.kind {
            case .separator:
                menu.addItem(.separator())
            case .submenu:
                guard let label = item.label, let children = item.items else { continue }
                let entry = NSMenuItem(title: label, action: nil, keyEquivalent: "")
                entry.isEnabled = item.enabled == true
                entry.submenu = build(children)
                loadIcon(item.icon, into: entry)
                menu.addItem(entry)
            case .command:
                guard let label = item.label, let id = item.id else { continue }
                let enabled = item.enabled == true
                let role = item.role
                let action: Selector? = enabled
                    ? role.map { NSSelectorFromString($0 + ":") } ?? #selector(select(_:))
                    : nil
                let entry = NSMenuItem(title: label, action: action, keyEquivalent: enabled ? item.key ?? "" : "")
                if role == nil, enabled {
                    entry.target = self
                    entry.representedObject = id
                }
                entry.isEnabled = enabled
                entry.state = item.checked == true ? .on : .off
                let modifiers = item.modifiers ?? []
                var mask: NSEvent.ModifierFlags = []
                if modifiers.contains("command") { mask.insert(.command) }
                if modifiers.contains("control") { mask.insert(.control) }
                if modifiers.contains("option") { mask.insert(.option) }
                if modifiers.contains("shift") { mask.insert(.shift) }
                entry.keyEquivalentModifierMask = mask
                loadIcon(item.icon, into: entry)
                menu.addItem(entry)
            }
        }
        return menu
    }
}

@MainActor
private struct JadixMenuBar {
    let main: NSMenu?
    let windows: NSMenu?
    let target: JadixMenuTarget?

    init(application: NSApplication) {
        main = application.mainMenu
        windows = application.windowsMenu
        target = nil
    }

    init(request: JadixMenuBarRequest, target: JadixMenuTarget) {
        let bar = NSMenu()
        bar.autoenablesItems = false
        var windowMenu: NSMenu?
        for (index, item) in request.items.enumerated() where item.kind != .separator {
            guard let label = item.label else { continue }
            let holder = NSMenuItem(title: label, action: nil, keyEquivalent: "")
            let menu = target.build(item.items ?? [item])
            menu.title = label
            holder.submenu = menu
            holder.isEnabled = item.kind == .command || item.enabled == true
            bar.addItem(holder)
            if index == request.windowMenu { windowMenu = menu }
        }
        main = bar
        windows = windowMenu
        self.target = target
    }

    func install(in application: NSApplication) {
        application.mainMenu = main
        application.windowsMenu = windows
    }
}

private struct JadixShareRequest: Decodable {
    let subject: String?
    let text: String?
    let url: String?
    let files: [String]
    let anchor: String?
    let anchorRoot: String?
}

@MainActor
private final class JadixSystemShare: NSObject, NSSharingServicePickerDelegate, NSSharingServiceDelegate {
    let id: UInt64
    private let picker: NSSharingServicePicker
    private let subject: String?
    private weak var owner: NSWindow?
    private var wasKey = false
    private var timer: Timer?
    private var completion: (([String: Any]) -> Void)?
    private var ticks = 0
    private var returnTicks = 0
    private var leftOwner = false

    init(id: UInt64, items: [Any], subject: String?, completion: @escaping ([String: Any]) -> Void) {
        self.id = id
        self.picker = NSSharingServicePicker(items: items)
        self.subject = subject
        self.completion = completion
        super.init()
        picker.delegate = self
    }

    func show(relativeTo rect: NSRect, of anchor: NSView) {
        owner = anchor.window
        wasKey = owner?.isKeyWindow == true
        picker.show(relativeTo: rect, of: anchor, preferredEdge: anchor.isFlipped ? .maxY : .minY)
    }

    func cancel(replaced: Bool = false) {
        finish(["Ok": ["status": replaced ? "replaced" : "dismissed"]])
        picker.close()
    }

    func sharingServicePicker(
        _ sharingServicePicker: NSSharingServicePicker, delegateFor sharingService: NSSharingService
    ) -> NSSharingServiceDelegate? {
        guard completion != nil else { return nil }
        if let subject { sharingService.subject = subject }
        return self
    }

    func sharingServicePicker(_ sharingServicePicker: NSSharingServicePicker, didChoose service: NSSharingService?) {
        guard completion != nil else { return }
        guard let service else {
            finish(["Ok": ["status": "dismissed"]])
            return
        }
        // Some system services never send a result. Track return to the originating window.
        let title = service.title
        let timer = Timer(timeInterval: 0.25, repeats: true) { [weak self] timer in
            MainActor.assumeIsolated {
                guard let self else { timer.invalidate(); return }
                self.checkForReturn(service: title)
            }
        }
        self.timer?.invalidate()
        self.timer = timer
        RunLoop.main.add(timer, forMode: .common)
        RunLoop.main.add(timer, forMode: .modalPanel)
    }

    func sharingService(_ sharingService: NSSharingService, didShareItems items: [Any]) {
        shared(with: sharingService.title)
    }

    func sharingService(_ sharingService: NSSharingService, didFailToShareItems items: [Any], error: Error) {
        let error = error as NSError
        if error.domain == NSCocoaErrorDomain, error.code == NSUserCancelledError {
            finish(["Ok": ["status": "dismissed"]])
        } else {
            finish(["Err": error.localizedDescription])
        }
    }

    private func checkForReturn(service: String) {
        ticks += 1
        let inOwner = owner.map { $0.isVisible && (!wasKey || $0.isKeyWindow) && $0.attachedSheet == nil } ?? true
        if NSApp.isActive, inOwner {
            returnTicks += 1
            if leftOwner ? returnTicks >= 2 : ticks >= 8 {
                shared(with: service)
                return
            }
        } else {
            leftOwner = true
            returnTicks = 0
        }
        if ticks >= 2_400 { finish(["Ok": ["status": "dismissed"]]) }
    }

    private func shared(with service: String) {
        finish(["Ok": ["status": "shared", "service": service]])
    }

    private func finish(_ result: [String: Any]) {
        guard let respond = completion else { return }
        completion = nil
        timer?.invalidate()
        timer = nil
        picker.delegate = nil
        respond(result)
    }
}

private struct JadixAlertRequest: Decodable {
    enum Operation: String, Decodable { case show, close }
    enum Style: String, Decodable { case informational, warning, critical }

    struct Input: Decodable {
        let value: String
        let placeholder: String?
        let accessibleName: String?
        let selectTextOnFocus: Bool?
        let required: Bool
    }

    struct Options: Decodable {
        let title: String
        let message: String
        let style: Style
        let buttons: [String]
        let defaultButton: Int?
        let cancelButton: Int?
        let input: Input?
    }

    let operation: Operation
    let alert: Options?
}

@MainActor
private func panelSheetOwner(_ window: NSWindow?) -> NSWindow? {
    guard let window, !NSApp.isHidden, window.isVisible,
          !window.isMiniaturized, window.screen != nil else { return nil }
    return window
}

@MainActor
private final class JadixSystemAlert: NSObject, NSTextFieldDelegate {
    private static var standaloneQueue: [JadixSystemAlert] = []
    private static var standaloneAlert: JadixSystemAlert?

    let id: UInt64
    let alert = NSAlert()
    private(set) weak var owner: NSWindow?
    private var field: NSTextField?
    private var requiresInput = false
    private var selectTextOnFocus = false
    private var completion: ((NSApplication.ModalResponse) -> Void)?
    private var cancelRequested = false
    private var cancelRetry: Timer?

    init(id: UInt64, options: JadixAlertRequest.Options) {
        self.id = id
        super.init()
        alert.messageText = options.title
        alert.informativeText = options.message
        switch options.style {
        case .informational: alert.alertStyle = .informational
        case .warning: alert.alertStyle = .warning
        case .critical: alert.alertStyle = .critical
        }
        for caption in options.buttons.isEmpty ? ["OK"] : options.buttons {
            alert.addButton(withTitle: caption)
        }
        if let defaultButton = options.defaultButton,
           alert.buttons.indices.contains(defaultButton), defaultButton != options.cancelButton {
            for button in alert.buttons where button.keyEquivalent == "\r" { button.keyEquivalent = "" }
            alert.buttons[defaultButton].keyEquivalent = "\r"
        }
        if let cancelButton = options.cancelButton,
           alert.buttons.indices.contains(cancelButton) {
            for (index, button) in alert.buttons.enumerated() where index != cancelButton {
                if button.keyEquivalent == "\u{1b}" { button.keyEquivalent = "" }
            }
            alert.buttons[cancelButton].keyEquivalent = "\u{1b}"
        }
        if let input = options.input {
            let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
            field.stringValue = input.value
            field.placeholderString = input.placeholder
            field.setAccessibilityLabel(input.accessibleName ?? input.placeholder ?? options.title)
            field.delegate = self
            self.field = field
            requiresInput = input.required
            selectTextOnFocus = input.selectTextOnFocus ?? false
            alert.accessoryView = field
            alert.window.initialFirstResponder = field
            updateButtonAvailability()
        }
    }

    var inputValue: String? {
        guard let field else { return nil }
        return field.currentEditor()?.string ?? field.stringValue
    }

    func controlTextDidChange(_ notification: Notification) {
        updateButtonAvailability()
    }

    private func updateButtonAvailability() {
        guard requiresInput else { return }
        let hasInput = inputValue?.isEmpty == false
        for button in alert.buttons {
            button.isEnabled = hasInput || button.keyEquivalent == "\u{1b}"
        }
    }

    @objc private func positionInitialSelection(_ notification: Notification) {
        guard let editor = field?.currentEditor() else { return }
        NotificationCenter.default.removeObserver(self, name: NSWindow.didBecomeKeyNotification, object: alert.window)
        let end = (editor.string as NSString).length
        editor.selectedRange = selectTextOnFocus ? NSRange(location: 0, length: end) : NSRange(location: end, length: 0)
    }

    func present(
        in window: NSWindow?,
        appearance: NSAppearance? = nil,
        completion: @escaping (NSApplication.ModalResponse) -> Void
    ) {
        self.completion = completion
        alert.window.appearance = appearance ?? window?.effectiveAppearance
        if field != nil {
            NotificationCenter.default.addObserver(
                self, selector: #selector(positionInitialSelection(_:)),
                name: NSWindow.didBecomeKeyNotification, object: alert.window
            )
        }
        owner = panelSheetOwner(window)
        if let owner {
            alert.beginSheetModal(for: owner) { [weak self] response in self?.finish(response) }
        } else {
            Self.standaloneQueue.append(self)
            Self.scheduleStandalone()
        }
    }

    private static func scheduleStandalone() {
        guard standaloneAlert == nil, !standaloneQueue.isEmpty else { return }
        // Enter the modal loop from the run loop so the main queue can still drain.
        RunLoop.main.perform(inModes: [.default, .modalPanel]) {
            MainActor.assumeIsolated {
                guard standaloneAlert == nil, !standaloneQueue.isEmpty else { return }
                let pending = standaloneQueue.removeFirst()
                standaloneAlert = pending
                NSApp.activate()
                let response = pending.cancelRequested ? .cancel : pending.alert.runModal()
                pending.finish(response)
                pending.alert.window.orderOut(nil)
                standaloneAlert = nil
                scheduleStandalone()
            }
        }
        CFRunLoopWakeUp(CFRunLoopGetMain())
    }

    func cancel() {
        guard completion != nil, !cancelRequested else { return }
        cancelRequested = true
        let sheet = alert.window
        if let owner, owner.sheets.contains(where: { $0 === sheet }) {
            owner.endSheet(sheet, returnCode: .cancel)
        } else if Self.standaloneAlert === self {
            if !stopStandalone() {
                // An embedding app may have opened a modal window above the alert.
                let timer = Timer(timeInterval: 0.1, repeats: true) { [weak self] _ in
                    MainActor.assumeIsolated { _ = self?.stopStandalone() }
                }
                cancelRetry = timer
                RunLoop.main.add(timer, forMode: .modalPanel)
            }
        } else {
            Self.standaloneQueue.removeAll { $0 === self }
            finish(.cancel)
        }
    }

    private func stopStandalone() -> Bool {
        guard NSApp.modalWindow === alert.window else { return false }
        cancelRetry?.invalidate()
        cancelRetry = nil
        NSApp.stopModal(withCode: .cancel)
        if let event = NSEvent.otherEvent(
            with: .applicationDefined, location: .zero, modifierFlags: [], timestamp: 0,
            windowNumber: 0, context: nil, subtype: 0, data1: 0, data2: 0
        ) {
            NSApp.postEvent(event, atStart: true)
        }
        return true
    }

    private func finish(_ response: NSApplication.ModalResponse) {
        cancelRetry?.invalidate()
        cancelRetry = nil
        if field != nil {
            NotificationCenter.default.removeObserver(self, name: NSWindow.didBecomeKeyNotification, object: alert.window)
        }
        let done = completion
        completion = nil
        done?(cancelRequested ? .cancel : response)
    }
}

/// Coordinates the optional exit decision used by an AppKit application's delegates.
@MainActor
public final class JadixExitConfirmation {
    public enum Reason {
        case quit
        case primaryWindowClose(NSWindow)
    }

    /// Resolve the decision on the main actor. Return an optional closure to dismiss its UI.
    public typealias Presenter = @MainActor (Reason, @escaping @MainActor (Bool) -> Void)
        -> (@MainActor () -> Void)?

    private final class Pending {
        weak var window: NSWindow?
        var application: NSApplication?
        var deadline: Timer?
        var dismiss: (@MainActor () -> Void)?

        init(window: NSWindow?) {
            self.window = window
        }
    }

    private let present: Presenter
    private let prepareToTerminate: (@MainActor (@escaping @MainActor () -> Void) -> Void)?
    private var pending: Pending?
    private var approvedQuit = false

    public init(
        prepareToTerminate: (@MainActor (@escaping @MainActor () -> Void) -> Void)? = nil,
        present: @escaping Presenter
    ) {
        self.prepareToTerminate = prepareToTerminate
        self.present = present
    }

    /// Call from the application's termination delegate when exit confirmation is enabled.
    public func applicationShouldTerminate(_ application: NSApplication) -> NSApplication.TerminateReply {
        if approvedQuit {
            guard let prepareToTerminate else { return .terminateNow }
            prepareToTerminate { application.reply(toApplicationShouldTerminate: true) }
            return .terminateLater
        }
        let request = pending ?? begin(.quit, window: nil)
        if request.application == nil {
            request.application = application
            let deadline = Timer(timeInterval: 110, repeats: false) { [weak self, weak request] _ in
                MainActor.assumeIsolated {
                    guard let request else { return }
                    self?.finish(request, allow: false)
                }
            }
            request.deadline = deadline
            RunLoop.main.add(deadline, forMode: .common)
            RunLoop.main.add(deadline, forMode: .modalPanel)
        }
        return .terminateLater
    }

    /// Treat a user-initiated primary window close as a request to quit.
    public func windowShouldClose(_ window: NSWindow) -> Bool {
        if pending == nil { _ = begin(.primaryWindowClose(window), window: window) }
        return false
    }

    /// Cancel a pending close when its target is removed by the host.
    public func windowWillClose(_ window: NSWindow) {
        guard let pending, pending.window === window else { return }
        finish(pending, allow: false)
    }

    private func begin(_ reason: Reason, window: NSWindow?) -> Pending {
        let request = Pending(window: window)
        pending = request
        RunLoop.main.perform(inModes: [.common, .modalPanel]) { [weak self, weak request] in
            MainActor.assumeIsolated {
                guard let self, let request, self.pending === request else { return }
                let dismiss = self.present(reason) { [weak self, weak request] allow in
                    guard let request else { return }
                    self?.finish(request, allow: allow)
                }
                if self.pending === request { request.dismiss = dismiss }
            }
        }
        CFRunLoopWakeUp(CFRunLoopGetMain())
        return request
    }

    private func finish(_ request: Pending, allow: Bool) {
        guard pending === request else { return }
        pending = nil
        request.deadline?.invalidate()
        request.dismiss?()
        if let application = request.application {
            if allow, let prepareToTerminate {
                prepareToTerminate { application.reply(toApplicationShouldTerminate: true) }
            } else {
                application.reply(toApplicationShouldTerminate: allow)
            }
        } else if allow, request.window != nil {
            approvedQuit = true
            defer { approvedQuit = false }
            NSApplication.shared.terminate(nil)
        }
    }
}

@MainActor
extension JadixExitConfirmation {
    /// Presents the standard AppKit quit alert using the host's alert queue.
    public static func presentSystemAlert(
        title: String, message: String, quitTitle: String, cancelTitle: String,
        in window: NSWindow?, resolve: @escaping @MainActor (Bool) -> Void
    ) -> @MainActor () -> Void {
        let options = JadixAlertRequest.Options(
            title: title, message: message, style: .warning,
            buttons: [quitTitle, cancelTitle], defaultButton: 0, cancelButton: 1, input: nil
        )
        let alert = JadixSystemAlert(id: 0, options: options)
        let sheetOwner = window.flatMap {
            $0.styleMask.contains(.titled) && $0.attachedSheet == nil ? $0 : nil
        }
        alert.present(in: sheetOwner, appearance: window?.effectiveAppearance) { response in
            resolve(response == .alertFirstButtonReturn)
        }
        return { alert.cancel() }
    }
}

private enum AppKitError: LocalizedError {
    case invalid(String)

    var errorDescription: String? {
        switch self {
        case let .invalid(message): return message
        }
    }
}

/// Application-owned content factories. Each invocation creates an independent Rust root.
@MainActor
public final class JadixWindowContentFactories {
    public typealias Factory = (JadixWindowRequest.Content) throws -> JadixNativeView.RootFactory
    private var factories: [String: Factory] = [:]

    public init() {}

    public func register(_ name: String, factory: @escaping Factory) throws {
        guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !name.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }),
              factories[name] == nil else {
            throw AppKitError.invalid("Invalid or duplicate window content factory: \(name)")
        }
        factories[name] = factory
    }

    public func resolve(_ content: JadixWindowRequest.Content) throws -> JadixNativeView.RootFactory {
        guard let factory = factories[content.factory] else {
            throw AppKitError.invalid("Unknown window content factory: \(content.factory)")
        }
        return try factory(content)
    }
}

public struct JadixWindowRequest: Decodable {
    public struct Content: Codable, Equatable {
        public let factory: String
        public let argument: String?

        public init(factory: String, argument: String? = nil) {
            self.factory = factory
            self.argument = argument
        }
    }
    public enum Kind: String, Decodable {
        case normal
        case dialog
        case panel
        case popup
        case overlay
    }

    public enum Operation: String, Decodable {
        case open
        case update
        case setMode = "set_mode"
        case restore
        case focus
        case setVisibility = "set_visibility"
        case close
        case openDeclared = "open_declared"
        case updateDeclared = "update_declared"
        case closeDeclared = "close_declared"
    }

    public enum Mode: String, Decodable {
        case normal, maximized, minimized
        case fullScreen = "full_screen"
    }

    public struct Attributes: Decodable {
        public enum TitleBar: String, Decodable {
            case standard, transparent, hidden
        }

        public enum Level: String, Decodable {
            case normal, floating
            case aboveDock = "above_dock"
            case desktopIcon = "desktop_icon"

            fileprivate var appKitLevel: NSWindow.Level {
                switch self {
                case .normal: return .normal
                case .floating: return .floating
                case .aboveDock: return .statusBar
                case .desktopIcon:
                    return NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.desktopIconWindow)))
                }
            }
        }

        public enum Workspaces: String, Decodable {
            case current, all
        }

        public enum Appearance: String, Decodable {
            case inherit, light, dark
        }

        public struct Size: Decodable {
            public let width: UInt32
            public let height: UInt32

            fileprivate var points: NSSize {
                NSSize(width: CGFloat(width), height: CGFloat(height))
            }
        }

        public struct Position: Decodable {
            public let x: Double
            public let y: Double

            fileprivate var points: NSPoint { NSPoint(x: CGFloat(x), y: CGFloat(y)) }
        }

        public let title: String?
        public let resizable: Bool?
        public let closeOnPress: Bool?
        public let closeOnEscape: Bool?
        public let closeOnBlur: Bool?
        public let dismissOnClose: Bool?
        public let level: Level?
        public let workspaces: Workspaces?
        public let participatesInCycle: Bool?
        public let appearance: Appearance?
        public let titleBar: TitleBar?
        public let contentSize: Size?
        public let minContentSize: Size?
        public let maxContentSize: Size?
        public let position: Position?
        public let mode: Mode?
        public let visible: Bool?

        private enum CodingKeys: String, CodingKey {
            case title, resizable, position, level, workspaces, appearance, mode, visible
            case closeOnPress = "close_on_press"
            case closeOnEscape = "close_on_escape"
            case closeOnBlur = "close_on_blur"
            case dismissOnClose = "dismiss_on_close"
            case participatesInCycle = "participates_in_cycle"
            case titleBar = "title_bar"
            case contentSize = "content_size"
            case minContentSize = "min_content_size"
            case maxContentSize = "max_content_size"
        }

        @MainActor
        public func apply(
            to window: NSWindow,
            preparePresentation: ((Mode?, NSWindow.StyleMask, NSWindow.CollectionBehavior) throws -> Void)? = nil,
            presentation: ((Mode?, Bool?) throws -> Void)? = nil
        ) throws {
            // Registry owners coordinate asynchronous full-screen transitions and
            // retained hidden roots. An embedded host must explicitly handle them.
            if (mode != nil || visible != nil) && presentation == nil {
                throw AppKitError.invalid("window mode and visibility require a presentation handler")
            }
            let minimum = minContentSize?.points ?? window.contentMinSize
            let maximum = maxContentSize?.points ?? window.contentMaxSize
            guard maximum.width > 0, maximum.height > 0,
                  minimum.width <= maximum.width, minimum.height <= maximum.height,
                  contentSize.map({ $0.width > 0 && $0.height > 0 }) ?? true else {
                throw AppKitError.invalid("invalid window content size or limits")
            }
            if titleBar != nil && !window.styleMask.contains(.titled) {
                throw AppKitError.invalid("title bar styles require a titled window")
            }
            if let position, !position.x.isFinite || !position.y.isFinite {
                throw AppKitError.invalid("invalid window position")
            }
            var style = window.styleMask
            if let resizable {
                if resizable { style.insert(.resizable) }
                else { style.remove(.resizable) }
            }
            if let titleBar {
                if titleBar == .standard { style.remove(.fullSizeContentView) }
                else { style.insert(.fullSizeContentView) }
            }
            var behavior = window.collectionBehavior
            var policy: Presentation?
            if level != nil || workspaces != nil || participatesInCycle != nil {
                let previous = Self.presentations.object(forKey: window)
                    ?? Presentation(defaultBehavior: behavior)
                let next = previous.updating(self)
                behavior = next.behavior(replacing: behavior)
                policy = next
            }
            try preparePresentation?(mode, style, behavior)

            if let title { window.title = title }
            if style != window.styleMask { window.styleMask = style }
            if let titleBar {
                window.titleVisibility = titleBar == .hidden ? .hidden : .visible
                window.titlebarAppearsTransparent = titleBar != .standard
                if let root = window.contentView as? JadixNativeView {
                    root.updateWindowEventMonitor()
                    root.scheduleWindowState()
                }
            }
            if let minContentSize { window.contentMinSize = minContentSize.points }
            if let maxContentSize { window.contentMaxSize = maxContentSize.points }
            if contentSize != nil || minContentSize != nil || maxContentSize != nil {
                let current = window.contentView?.frame.size ?? window.contentRect(forFrameRect: window.frame).size
                let requested = contentSize?.points ?? current
                let size = NSSize(
                    width: min(max(requested.width, minimum.width), maximum.width),
                    height: min(max(requested.height, minimum.height), maximum.height)
                )
                if size != current { window.setContentSize(size) }
            }
            if let position { window.setFrameOrigin(position.points) }
            if let appearance {
                switch appearance {
                case .inherit: window.appearance = nil
                case .light: window.appearance = NSAppearance(named: .aqua)
                case .dark: window.appearance = NSAppearance(named: .darkAqua)
                }
            }
            if let policy {
                Self.presentations.setObject(policy, forKey: window)
                if let level { window.level = level.appKitLevel }
                if behavior != window.collectionBehavior { window.collectionBehavior = behavior }
            }
            if mode != nil || visible != nil { try presentation?(mode, visible) }
        }

        @MainActor private static let presentations =
            NSMapTable<NSWindow, Presentation>.weakToStrongObjects()

        @MainActor
        private final class Presentation {
            let defaultBehavior: NSWindow.CollectionBehavior
            var level: Level?
            var workspaces: Workspaces?
            var participatesInCycle: Bool?

            init(defaultBehavior: NSWindow.CollectionBehavior) {
                self.defaultBehavior = defaultBehavior
            }

            func updating(_ attributes: Attributes) -> Presentation {
                let next = Presentation(defaultBehavior: defaultBehavior)
                next.level = attributes.level ?? level
                next.workspaces = attributes.workspaces ?? workspaces
                next.participatesInCycle = attributes.participatesInCycle ?? participatesInCycle
                return next
            }

            func behavior(replacing current: NSWindow.CollectionBehavior) -> NSWindow.CollectionBehavior {
                // Keep host flags unrelated to this policy, including tiling.
                let spaceFlags: NSWindow.CollectionBehavior = [.canJoinAllSpaces, .moveToActiveSpace]
                let fullScreenFlags: NSWindow.CollectionBehavior = [
                    .fullScreenPrimary, .fullScreenAuxiliary, .fullScreenNone,
                ]
                let missionControlFlags: NSWindow.CollectionBehavior = [.managed, .transient, .stationary]
                let cycleFlags: NSWindow.CollectionBehavior = [.participatesInCycle, .ignoresCycle]
                let managedFlags = spaceFlags.union(fullScreenFlags).union(missionControlFlags).union(cycleFlags)
                var behavior = current.subtracting(managedFlags)
                if level == .desktopIcon {
                    behavior.formUnion([.stationary, .canJoinAllSpaces, .ignoresCycle])
                } else {
                    behavior.formUnion(defaultBehavior.intersection(missionControlFlags))
                    if workspaces == .all || (workspaces == nil && defaultBehavior.contains(.canJoinAllSpaces)) {
                        behavior.insert(.canJoinAllSpaces)
                        if workspaces == .all {
                            behavior.insert(.fullScreenAuxiliary)
                        } else {
                            behavior.formUnion(defaultBehavior.intersection(fullScreenFlags))
                        }
                    } else {
                        if level == .floating || level == .aboveDock {
                            behavior.insert(.moveToActiveSpace)
                        } else if level == nil, workspaces == nil {
                            behavior.formUnion(defaultBehavior.intersection(spaceFlags))
                        }
                        let defaultFullScreen = workspaces == .current
                            ? fullScreenFlags.subtracting(.fullScreenAuxiliary) : fullScreenFlags
                        behavior.formUnion(defaultBehavior.intersection(defaultFullScreen))
                    }
                    if let participatesInCycle {
                        behavior.insert(participatesInCycle ? .participatesInCycle : .ignoresCycle)
                    } else {
                        behavior.formUnion(defaultBehavior.intersection(cycleFlags))
                    }
                }
                return behavior
            }
        }
    }

    public let operation: Operation
    public let id: String
    public let kind: Kind?
    public let parent: String?
    public let title: String?
    public let width: UInt32?
    public let height: UInt32?
    public let resizable: Bool?
    public let closeOnBlur: Bool?
    public let modal: Bool?
    public let initialMode: Mode?
    public let initiallyVisible: Bool?
    public let attributes: Attributes?
    public let content: Content?
    public let mode: Mode?
    public let visible: Bool?

    private enum CodingKeys: String, CodingKey {
        case operation, id, kind, parent, title, width, height, resizable, attributes, mode, visible, modal, content
        case closeOnBlur = "close_on_blur"
        case initialMode = "initial_mode"
        case initiallyVisible = "initially_visible"
    }
}

public enum JadixWindowEvent {
    public enum CloseReason: String {
        case closeButton = "close_button"
        case shortcut, menu
    }

    public enum CloseCause: String {
        case closePress = "close_press"
        case escape, blur
        case parentClose = "parent_close"
        case close
    }

    case closePress(CloseReason)
    case closed(CloseCause)
    case childDestroyed(String)
    case childState(String, [String: Any]?)
    case quitRequest

    fileprivate var payload: [String: Any] {
        switch self {
        case let .closePress(reason):
            return ["event": "close_press", "reason": reason.rawValue]
        case let .closed(cause):
            return ["event": "closed", "cause": cause.rawValue]
        case let .childDestroyed(id):
            return ["event": "child_destroyed", "id": id]
        case let .childState(id, state):
            return ["event": "child_state", "id": id, "state": state as Any? ?? NSNull()]
        case .quitRequest:
            return ["event": "quit_request"]
        }
    }
}


private struct JadixAccessibilityGridCell {
    let rows: Range<Int>
    let columns: Range<Int>
    let view: NSView
}

private struct JadixAccessibilityGridHeader {
    let columns: Range<Int>
    let view: NSView
}

private final class JadixContainerView: NSView {
    var accessibilityGridCells: [JadixAccessibilityGridCell]?
    var accessibilityGridHeaders: [JadixAccessibilityGridHeader] = []
    private var gridRows: [Int: JadixAccessibilityRow] = [:]
    private var gridColumns: [Int: JadixAccessibilityColumn] = [:]
    private var logicalRows = 0
    private var logicalColumns = 0

    func setGridAxes(rows: [Int: JadixAccessibilityRow], columns: [Int: JadixAccessibilityColumn],
                     rowCount: Int, columnCount: Int) {
        for (index, row) in gridRows where rows[index] !== row { row.clear() }
        for (index, column) in gridColumns where columns[index] !== column {
            column.cells.removeAll(); column.setAccessibilityChildren(nil); column.setAccessibilityParent(nil)
        }
        gridRows = rows; gridColumns = columns
        logicalRows = rowCount; logicalColumns = columnCount
    }

    // Empty axes are materialized on AX queries, never in the render/publish path.
    override func accessibilityRows() -> [Any]? {
        guard accessibilityGridCells != nil else { return super.accessibilityRows() }
        return (0..<logicalRows).map { index in
            if let row = gridRows[index] { return row }
            let row = JadixAccessibilityRow()
            let cells = accessibilityGridCells?.filter { $0.rows.contains(index) }.map(\.view) ?? []
            row.update(parent: self, index: index, cells: cells, ownsCells: false)
            gridRows[index] = row
            return row
        }
    }

    override func accessibilityChildren() -> [Any]? {
        guard accessibilityGridCells != nil else { return super.accessibilityChildren() }
        return accessibilityRows()
    }

    override func accessibilityColumns() -> [Any]? {
        guard accessibilityGridCells != nil else { return super.accessibilityColumns() }
        return (0..<logicalColumns).map { index in
            if let column = gridColumns[index] { return column }
            let column = JadixAccessibilityColumn()
            let headers = accessibilityGridHeaders.filter { $0.columns.contains(index) }.map(\.view)
            let cells = headers + (accessibilityGridCells?.filter { $0.columns.contains(index) }.map(\.view) ?? [])
            column.cells = cells
            column.setAccessibilityElement(true); column.setAccessibilityRole(.column)
            column.setAccessibilityIndex(index); column.setAccessibilityParent(self)
            column.setAccessibilityChildren(cells); column.setAccessibilityHeader(headers.first)
            let frame = cells.reduce(CGRect.null) { $0.union(convert($1.bounds, from: $1)) }
            column.setAccessibilityFrameInParentSpace(frame.isNull ? .zero : frame)
            gridColumns[index] = column
            return column
        }
    }

    func clearAccessibilityGrid() {
        accessibilityGridCells = nil
        accessibilityGridHeaders.removeAll()
        for row in gridRows.values { row.clear() }
        for column in gridColumns.values { column.cells.removeAll(); column.setAccessibilityChildren(nil); column.setAccessibilityParent(nil) }
        gridRows.removeAll(); gridColumns.removeAll()
        logicalRows = 0; logicalColumns = 0
        setAccessibilityHeader(nil)
    }

    override var isFlipped: Bool { true }

    override func accessibilityCell(forColumn column: Int, row: Int) -> Any? {
        guard let cells = accessibilityGridCells else {
            return super.accessibilityCell(forColumn: column, row: row)
        }
        if let cell = cells.first(where: { $0.rows.contains(row) && $0.columns.contains(column) }) {
            return cell.view
        }
        guard row == 0 else { return nil }
        return accessibilityGridHeaders.first { $0.columns.contains(column) }?.view
    }

    override func accessibilityVisibleCells() -> [Any]? {
        guard accessibilityGridCells != nil else { return super.accessibilityVisibleCells() }
        return accessibilityGridCells?.map(\.view).filter(\.isVisibleToGridAccessibility)
    }

    override func accessibilityVisibleRows() -> [Any]? {
        guard accessibilityGridCells != nil else { return super.accessibilityVisibleRows() }
        return gridRows.values.filter(\.hasVisibleCells).sorted { $0.accessibilityIndex() < $1.accessibilityIndex() }
    }

    override func accessibilityVisibleColumns() -> [Any]? {
        guard accessibilityGridCells != nil else { return super.accessibilityVisibleColumns() }
        return gridColumns.values.filter(\.hasVisibleCells).sorted { $0.accessibilityIndex() < $1.accessibilityIndex() }
    }
}

private class JadixOverlayView: NSView {
    let contentHost = JadixContainerView()
    var panelFrame = NSRect.zero
    var isOpen = false
    var isModal = false
    var closeOnOutside = false
    var onDismiss: ((Bool) -> Void)?

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        setAccessibilityElement(true)
        addSubview(contentHost)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let localPoint = convert(point, from: superview)
        guard isOpen, !isHidden, bounds.contains(localPoint) else { return nil }
        if panelFrame.contains(localPoint) {
            return contentHost.hitTest(localPoint) ?? contentHost
        }
        return isModal ? self : nil
    }

    override func mouseDown(with event: NSEvent) {
        guard isModal, closeOnOutside else { return }
        let point = convert(event.locationInWindow, from: nil)
        if !panelFrame.contains(point) { onDismiss?(true) }
    }
}

private final class JadixDialogView: JadixOverlayView {
    var backdropAlpha: CGFloat = 0.32 { didSet { needsDisplay = true } }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.black.withAlphaComponent(backdropAlpha).setFill()
        bounds.fill()
    }
}

private final class JadixPopoverView: JadixOverlayView {

    weak var anchor: NSView?
    var closeOnEscape = true
    var reportedOpen = false
    var reportedPosition: JadixPopoverEvent.Position?
    var enterDuration: TimeInterval = 0
    var exitDuration: TimeInterval = 0
    var startingAppearance = JadixPopoverAppearance()
    var endingAppearance = JadixPopoverAppearance()
    var appearanceWire: [String: Any] = [:]
    var appearanceBackground = NSColor.clear
    var visibleAppearance = JadixVisualStyle()
    private lazy var visuals = JadixViewAppearance(contentHost)
    private(set) var isClosing = false
    private var isEntering = false
    private var animationRevision: UInt64 = 0

    func updateAppearance() {
        guard !isHidden else { return }
        if isEntering || isClosing { visuals.render(visuals.current) }
        else { visuals.update(appearanceWire, background: appearanceBackground) }
    }

    func showPanel() {
        guard isHidden || isClosing else { updateAppearance(); return }
        let from = isHidden ? startingAppearance.resolved(over: visibleAppearance) : visuals.current
        animationRevision &+= 1
        let revision = animationRevision
        isEntering = true; isClosing = false; isHidden = false; setAccessibilityHidden(false)
        visuals.animate(from: from, to: visibleAppearance, duration: enterDuration) { [weak self] in
            guard let self, self.animationRevision == revision, !self.isClosing else { return }
            self.isEntering = false; self.updateAppearance()
        }
    }

    func hidePanel() {
        animationRevision &+= 1
        visuals.cancel(); isEntering = false; isClosing = false; isHidden = true; setAccessibilityHidden(true)
    }

    func closePanel(completion: @escaping () -> Void) -> Bool {
        guard !isHidden, window != nil, exitDuration > 0,
              !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else {
            hidePanel(); return false
        }
        animationRevision &+= 1
        let revision = animationRevision
        isEntering = false; isClosing = true; setAccessibilityHidden(true)
        visuals.animate(to: endingAppearance.resolved(over: visibleAppearance), duration: exitDuration) { [weak self] in
            guard let self, self.animationRevision == revision, self.isClosing, !self.isOpen else { return }
            self.hidePanel(); completion()
        }
        return true
    }
}

private extension NSView {
    var isVisibleToGridAccessibility: Bool { !isHiddenOrHasHiddenAncestor && !visibleRect.isEmpty }
}

private final class JadixAccessibilityRow: NSAccessibilityElement, NSAccessibilityRow {
    private var cells: [NSView] = []
    private var rowIndex = 0

    override func accessibilityIndex() -> Int { rowIndex }
    override func accessibilityVisibleCells() -> [Any]? { cells.filter(\.isVisibleToGridAccessibility) }
    override func accessibilityDisclosureLevel() -> Int { 0 }

    var hasVisibleCells: Bool { cells.contains(where: \.isVisibleToGridAccessibility) }

    func update(parent: NSView, index: Int, cells nextCells: [NSView], ownsCells: Bool = true) {
        let nextIdentities = Set(nextCells.map(ObjectIdentifier.init))
        for cell in cells where !nextIdentities.contains(ObjectIdentifier(cell)) {
            restoreParent(of: cell)
        }
        cells = nextCells
        rowIndex = index
        setAccessibilityElement(true)
        setAccessibilityRole(.row)
        setAccessibilityParent(parent)
        setAccessibilityChildren(nextCells)
        let frame = nextCells.reduce(CGRect.null) { frame, cell in
            frame.union(parent.convert(cell.bounds, from: cell))
        }
        setAccessibilityFrameInParentSpace(frame.isNull ? .zero : frame)
        if ownsCells { for cell in nextCells { cell.setAccessibilityParent(self) } }
    }

    func clear() {
        for cell in cells { restoreParent(of: cell) }
        cells.removeAll()
        setAccessibilityChildren(nil)
        setAccessibilityParent(nil)
    }

    private func restoreParent(of cell: NSView) {
        if (cell.accessibilityParent() as? JadixAccessibilityRow) === self {
            cell.setAccessibilityParent(cell.superview)
        }
    }
}

private final class JadixAccessibilityColumn: NSAccessibilityElement {
    var cells: [NSView] = []
    var hasVisibleCells: Bool { cells.contains(where: \.isVisibleToGridAccessibility) }

    override func accessibilityVisibleCells() -> [Any]? { cells.filter(\.isVisibleToGridAccessibility) }
}

private final class JadixRadioButton: NSButton {
    override var isFlipped: Bool { true }
    // Content is decorative; the common control retains activation and focus.
    override func hitTest(_ point: NSPoint) -> NSView? {
        super.hitTest(point) == nil ? nil : self
    }
    var takesKeyboardFocus = true
    var activatesOnPress = false
    override var acceptsFirstResponder: Bool { takesKeyboardFocus && super.acceptsFirstResponder }
    override func mouseDown(with event: NSEvent) {
        if activatesOnPress {
            if isEnabled { _ = sendAction(action, to: target) }
            return
        }
        super.mouseDown(with: event)
    }
    var moveInGroup: ((UInt16) -> Bool)?
    var pointerPresence: ((Bool) -> Void)?
    var pointerMotion: ((NSPoint) -> Void)?
    private var pointerTracking: NSTrackingArea?

    override func updateTrackingAreas() {
        if let pointerTracking { removeTrackingArea(pointerTracking) }
        super.updateTrackingAreas()
        let area = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self, userInfo: nil)
        addTrackingArea(area)
        pointerTracking = area
    }
    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        if isEnabled { pointerMotion?(event.locationInWindow) }
    }
    override func mouseEntered(with event: NSEvent) {
        super.mouseEntered(with: event)
        if isEnabled { pointerPresence?(true) }
    }
    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        if isEnabled { pointerPresence?(false) }
    }

    override func keyDown(with event: NSEvent) {
        if [123, 124, 125, 126].contains(event.keyCode), moveInGroup?(event.keyCode) == true { return }
        super.keyDown(with: event)
    }
}

@MainActor
private final class AppleImagePlayback {
    private weak var view: NSView?
    private weak var layer: CALayer?
    private let sequence: AppleImageSequence
    private let first: CGImage
    private var timer: Timer?
    private var deadline: CFTimeInterval
    private var nextIndex = 1
    private var ready: (CGImage, Double)?
    private var running = true
    private var paused = false

    init(view: NSView, layer: CALayer, sequence: AppleImageSequence, first: CGImage) {
        self.view = view; self.layer = layer; self.sequence = sequence; self.first = first
        deadline = CACurrentMediaTime() + sequence.firstDelay
        prefetch()
        let timer = Timer(timeInterval: 1.0 / 120, repeats: true) { [weak self] _ in self?.tick() }
        self.timer = timer
        RunLoop.main.add(timer, forMode: .common)
    }
    deinit { timer?.invalidate() }
    func stop() {
        running = false; timer?.invalidate(); timer = nil; ready = nil
        layer?.contents = first
    }
    private func prefetch() {
        let index = nextIndex, sequence = sequence
        appleImageQueue.async { [weak self] in
            let frame = sequence.frame(index)
            DispatchQueue.main.async { [weak self] in
                guard let self, self.running else { return }
                guard let frame else { self.stop(); return }
                self.ready = frame
            }
        }
    }
    private func tick() {
        guard running, let view, let layer, let window = view.window else { stop(); return }
        let now = CACurrentMediaTime()
        if NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            layer.contents = first; paused = true; return
        }
        if view.isHiddenOrHasHiddenAncestor || view.visibleRect.isEmpty || !window.occlusionState.contains(.visible) {
            paused = true; return
        }
        if paused { deadline = now + sequence.firstDelay; paused = false }
        guard now >= deadline, let (frame, delay) = ready else { return }
        ready = nil
        CATransaction.begin(); CATransaction.setDisableActions(true)
        layer.contents = frame
        CATransaction.commit()
        deadline = max(deadline + delay, now)
        nextIndex = (nextIndex + 1) % sequence.count
        prefetch()
    }
}

private final class JadixImageView: NSImageView {
    private var source: String?
    private var placeholder: String?
    private var placeholderRequest: UUID?
    private var placeholderLoad: (url: URL, token: UUID)?
    private var showingMainImage = false
    private var fadeSeconds = 0.15
    private var blurRadius: CGFloat = 0
    private var fit: AppleImageFit = .contain
    private var fraction = CGPoint(x: 0.5, y: 0.5)
    private var offset = CGPoint.zero
    private var sourceRevision: UInt64 = 0
    private var loadStarted = false
    private var loadEnded = false
    private var reportedBytes = 0
    private var lastRequest: AppleImageRequest?
    private var decodeID: UUID?
    private var remoteLoad: (url: URL, token: UUID)?
    private let remoteImages: AppleRemoteImages
    private var originalSize: NSSize?
    private var displayedImage: AppleDecodedImage?
    private var playbackLayer: CALayer?
    private var playback: AppleImagePlayback?
    var onOriginalSizeChange: (() -> Void)?
    var onLoadEvent: (([String: Any]) -> Void)?

    init(remoteImages: AppleRemoteImages) {
        self.remoteImages = remoteImages
        super.init(frame: .zero)
    }

    required init?(coder: NSCoder) { nil }

    func originalSize(for requestedSource: String) -> NSSize? {
        source == requestedSource ? originalSize : nil
    }

    func setSource(_ next: String?, style: [String: Any]? = nil) {
        let nextPlaceholder = style?["image_placeholder"] as? String
        if source != next {
            cancelDecode()
            cancelPlaceholder()
            source = next
            sourceRevision &+= 1
            loadStarted = false
            loadEnded = false
            reportedBytes = 0
            lastRequest = nil
            originalSize = nil
            clearImage()
        } else if placeholder != nextPlaceholder {
            cancelPlaceholder()
            if !showingMainImage { clearImage() }
        }
        placeholder = nextPlaceholder
        fadeSeconds = (style?["image_fade_ms"] as? Double).map { $0 / 1000 } ?? 0.15
        blurRadius = CGFloat(style?["image_blur_radius"] as? Double ?? 0)
        fit = (style?["image_fit"] as? String).flatMap(AppleImageFit.init(rawValue:)) ?? .contain
        let position = style?["image_position"] as? [String: Any]
        fraction = imagePositionPoint(position?["fraction"]) ?? CGPoint(x: 0.5, y: 0.5)
        offset = imagePositionPoint(position?["offset"]) ?? .zero
        layoutImage()
        updatePlayback()
        decodeLatest()
        decodePlaceholder()
    }

    private func clearImage() {
        playback?.stop(); playback = nil
        showingMainImage = false
        displayedImage = nil
        playbackLayer?.removeFromSuperlayer()
        playbackLayer = nil
        layer?.removeAnimation(forKey: "image-load")
    }

    private func imagePositionPoint(_ value: Any?) -> CGPoint? {
        guard let pair = value as? [Double], pair.count == 2,
              pair[0].isFinite, pair[1].isFinite else { return nil }
        return CGPoint(x: pair[0], y: pair[1])
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            if decodeID != nil { lastRequest = nil }
            cancelDecode()
            if displayedImage == nil { cancelPlaceholder() }
        }
        updatePlayback()
        decodeLatest()
        decodePlaceholder()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        refreshBackingDensity()
    }

    func refreshBackingDensity() {
        decodeLatest()
        decodePlaceholder()
    }

    override func layout() {
        super.layout()
        layoutImage()
    }

    private func layoutImage() {
        guard let playbackLayer, let backingLayer = playbackLayer.superlayer,
              let picture = displayedImage else { return }
        let bounds = backingLayer.bounds
        let natural = picture.originalSize
        let scaleX = bounds.width / natural.width
        let scaleY = bounds.height / natural.height
        let scale: CGFloat
        switch fit {
        case .contain: scale = min(scaleX, scaleY)
        case .cover: scale = max(scaleX, scaleY)
        case .fill, .none: scale = 1
        case .scaleDown: scale = min(1, scaleX, scaleY)
        }
        let size = fit == .fill ? bounds.size : NSSize(width: natural.width * scale, height: natural.height * scale)
        let x = (bounds.width - size.width) * fraction.x + offset.x
        let y = (bounds.height - size.height) * fraction.y + offset.y
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        playbackLayer.filters = blurRadius > 0 ? [CIFilter(name: "CIGaussianBlur", parameters: ["inputRadius": blurRadius])].compactMap { $0 } : nil
        playbackLayer.frame = NSRect(x: bounds.minX + x,
                                     y: backingLayer.isGeometryFlipped ? bounds.minY + y : bounds.maxY - size.height - y,
                                     width: size.width, height: size.height)
        CATransaction.commit()
    }

    private func updatePlayback() {
        guard showingMainImage, let layer = playbackLayer, let picture = displayedImage,
              let sequence = picture.sequence, let first = picture.frames.first,
              window != nil else {
            playback?.stop(); playback = nil
            return
        }
        if playback == nil { playback = AppleImagePlayback(view: self, layer: layer, sequence: sequence, first: first) }
    }

    private func display(_ picture: AppleDecodedImage, isMain: Bool) {
        guard let backingLayer = layer, let first = picture.frames.first else { return }
        if isMain, !showingMainImage, fadeSeconds > 0,
           source?.hasPrefix(appleInlineImagePrefix) == false,
           !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            let fade = CATransition()
            fade.type = .fade
            fade.duration = fadeSeconds
            backingLayer.add(fade, forKey: "image-load")
        }
        playback?.stop(); playback = nil
        showingMainImage = isMain
        displayedImage = picture
        playbackLayer?.removeFromSuperlayer()
        let playback = CALayer()
        backingLayer.masksToBounds = true
        playback.contents = first
        backingLayer.addSublayer(playback)
        playbackLayer = playback
        layoutImage()
        updatePlayback()
    }

    private var imageRequest: AppleImageRequest? {
        guard let source, window != nil else { return nil }
        let density = jadixBackingDensity(self)
        let pixels = NSSize(width: bounds.width * density, height: bounds.height * density)
        guard pixels.width.isFinite, pixels.height.isFinite else { return nil }
        return AppleImageRequest(
            source: source,
            pixelWidth: Int((min(max(pixels.width, 64), 4096) / 64).rounded(.up)) * 64,
            pixelHeight: Int((min(max(pixels.height, 64), 4096) / 64).rounded(.up)) * 64,
            fit: fit
        )
    }

    private func cancelDecode() {
        decodeID = nil
        if let remoteLoad {
            remoteImages.cancel(remoteLoad.url, token: remoteLoad.token)
            self.remoteLoad = nil
        }
    }

    private func cancelPlaceholder() {
        if let load = placeholderLoad { remoteImages.cancel(load.url, token: load.token) }
        placeholderLoad = nil
        placeholderRequest = nil
    }

    private func decodePlaceholder() {
        guard !showingMainImage, placeholderRequest == nil, let placeholder,
              let main = imageRequest, placeholder != main.source else { return }
        let revision = UUID()
        placeholderRequest = revision
        let request = AppleImageRequest(source: placeholder, pixelWidth: main.pixelWidth,
                                         pixelHeight: main.pixelHeight, fit: .cover)
        placeholderLoad = remoteImages.decode(request, completion: { [weak self] decoded in
            guard let self, self.placeholderRequest == revision else { return }
            self.placeholderLoad = nil
            if let decoded, !self.showingMainImage { self.display(decoded, isMain: false) }
        })
    }

    private func decodeLatest() {
        guard decodeID == nil, let request = imageRequest, request != lastRequest else { return }
        let id = UUID()
        lastRequest = request
        decodeID = id
        let revision = sourceRevision
        if !loadStarted {
            loadStarted = true
            reportLoadEvents([["kind": "imageLoadStart"]], revision: revision)
        }
        remoteLoad = remoteImages.decode(request) { [weak self] decoded in
            guard let self, self.decodeID == id else { return }
            self.remoteLoad = nil
            self.decodeID = nil
            self.finishDecode(request, revision: revision, decoded: decoded)
        } networkFinished: { [weak self] in
            guard let self, self.decodeID == id else { return }
            self.remoteLoad = nil
        } progress: { [weak self] loaded, total in
            guard let self, self.decodeID == id, !self.loadEnded, loaded > self.reportedBytes else { return }
            self.reportedBytes = loaded
            self.reportLoadEvents([["kind": "imageProgress", "loaded": loaded, "total": total]], revision: revision)
        }
    }

    private func finishDecode(_ request: AppleImageRequest, revision: UInt64, decoded: AppleDecodedImage?) {
        if sourceRevision == revision, let current = imageRequest {
            if let decoded {
                if originalSize != decoded.originalSize {
                    originalSize = decoded.originalSize
                    onOriginalSizeChange?()
                }
                if current == request {
                    cancelPlaceholder()
                    display(decoded, isMain: true)
                }
            }
            if current == request, !loadEnded {
                loadEnded = true
                let result: [String: Any]
                if let decoded {
                    result = ["kind": "imageLoad", "width": Int(decoded.originalSize.width),
                              "height": Int(decoded.originalSize.height)]
                } else {
                    result = ["kind": "imageError", "message": "Unable to load or decode the image."]
                }
                reportLoadEvents([result, ["kind": "imageLoadEnd"]], revision: revision)
            }
        }
        decodeLatest()
    }

    private func reportLoadEvents(_ events: [[String: Any]], revision: UInt64) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            for event in events {
                guard self.sourceRevision == revision else { return }
                self.onLoadEvent?(event)
            }
        }
    }
}

private final class JadixLabel: NSTextField {
    var textAppearance: JadixTextAppearance? { didSet { needsDisplay = true } }
    private static let bidiMarkers = try? NSRegularExpression(
        pattern: #"([\u2066-\u2068])|(\u2069)|(\p{bc=L})|[\p{bc=R}\p{bc=AL}]"#
    )
    private static let hardLineBreaks = try? NSRegularExpression(
        pattern: #"\r\n|[\n\r\u000b\u000c\u0085\u2028\u2029]"#
    )

    var overflowLineLimit: Int? {
        didSet { needsDisplay = true }
    }
    var truncation: CTLineTruncationType? {
        didSet { needsDisplay = true }
    }

    static func startsRightToLeft(_ text: String, range: NSRange) -> Bool {
        var isolates = 0
        var rtl = false
        bidiMarkers?.enumerateMatches(in: text, range: range) { match, _, stop in
            guard let match else { return }
            if match.range(at: 1).location != NSNotFound {
                isolates += 1
            } else if match.range(at: 2).location != NSNotFound {
                isolates = max(0, isolates - 1)
            } else if isolates == 0 {
                rtl = match.range(at: 3).location == NSNotFound
                stop.pointee = true
            }
        }
        return rtl
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        if overflowLineLimit != nil || textAppearance != nil { needsDisplay = true }
    }

    func overflowLayout(width: CGFloat, height: CGFloat) -> (lines: [OverflowLine], size: NSSize)? {
        guard overflowLineLimit != nil || textAppearance != nil,
              width > 0, width.isFinite, height > 0 else { return nil }
        let limit = overflowLineLimit ?? (maximumNumberOfLines > 0 ? maximumNumberOfLines : Int.max)
        let attributed = attributedStringValue
        guard attributed.length > 0 else { return nil }
        let source = attributed.string as NSString
        let text = NSMutableAttributedString(attributedString: attributed)
        let fullRange = NSRange(location: 0, length: text.length)
        let paragraphKey = NSAttributedString.Key(kCTParagraphStyleAttributeName as String)
        let colorKey = NSAttributedString.Key(kCTForegroundColorAttributeName as String)
        attributed.enumerateAttribute(.foregroundColor, in: fullRange) { value, range, _ in
            if let color = value as? NSColor {
                text.addAttribute(colorKey, value: color.cgColor, range: range)
            }
        }
        attributed.enumerateAttribute(.underlineStyle, in: fullRange) { value, range, _ in
            if let value = value as? Int, value != 0 { text.addAttribute(.jadixUnderline, value: true, range: range) }
        }
        text.removeAttribute(.underlineStyle, range: fullRange)
        let left = Self.coreTextParagraph(rtl: false)
        let right = Self.coreTextParagraph(rtl: true)
        attributed.enumerateAttribute(.paragraphStyle, in: fullRange) { value, range, _ in
            let direction = (value as? NSParagraphStyle)?.baseWritingDirection ?? .natural
            var start = range.location
            while start < NSMaxRange(range) {
                let paragraphRange = source.paragraphRange(for: NSRange(location: start, length: 0))
                let segment = NSIntersectionRange(range, paragraphRange)
                let rtl = direction == .rightToLeft || (direction == .natural && Self.startsRightToLeft(source as String, range: paragraphRange))
                text.addAttribute(paragraphKey, value: rtl ? right : left, range: segment)
                start = NSMaxRange(segment)
            }
        }
        let typesetter = CTTypesetterCreateWithAttributedString(text as CFAttributedString)
        var lines: [OverflowLine] = []
        var nextStart = 0
        var lastStart = 0
        var lineTop: CGFloat = 0
        func isRightToLeft(at start: Int) -> Bool {
            let direction = (attributed.attribute(.paragraphStyle, at: start, effectiveRange: nil)
                             as? NSParagraphStyle)?.baseWritingDirection ?? .natural
            return direction == .rightToLeft || (direction == .natural && Self.startsRightToLeft(source as String,
                range: source.paragraphRange(for: NSRange(location: start, length: 0))))
        }
        func paragraph(at start: Int, rtl: Bool) -> NSParagraphStyle? {
            let paragraph = (attributed.attribute(.paragraphStyle, at: start, effectiveRange: nil) as? NSParagraphStyle)?.mutableCopy() as? NSMutableParagraphStyle
            if let align = textAppearance?.values["align"] as? String {
                if align == "start" { paragraph?.alignment = rtl ? .right : .left }
                if align == "end" { paragraph?.alignment = rtl ? .left : .right }
            }
            return paragraph
        }
        while nextStart < text.length && lines.count < limit {
            let count = CTTypesetterSuggestLineBreak(typesetter, nextStart, Double(width))
            let length = count > 0 ? count : source.rangeOfComposedCharacterSequence(at: nextStart).length
            var line = CTTypesetterCreateLine(typesetter, CFRange(location: nextStart, length: length))
            let rtl = isRightToLeft(at: nextStart)
            let paragraph = paragraph(at: nextStart, rtl: rtl)
            let paragraphEnd = NSMaxRange(source.paragraphRange(for: NSRange(location: nextStart, length: 0)))
            if paragraph?.alignment == .justified, nextStart + length < paragraphEnd,
               let justified = CTLineCreateJustifiedLine(line, 1, Double(width)) { line = justified }
            let row = OverflowLine(line, top: lineTop, width: width, rtl: isRightToLeft(at: nextStart), paragraph: paragraph)
            // Keep the first line at its normal position even in a box shorter than its font metrics.
            if !lines.isEmpty && row.bottom > height { break }
            lines.append(row)
            lastStart = nextStart
            nextStart += length
            lineTop = row.bottom
        }
        if overflowLineLimit != nil, let last = lines.last,
           nextStart < text.length || (truncation != nil && last.width > width) {
            let rtl = isRightToLeft(at: lastStart)
            let remainder = NSRange(location: lastStart, length: text.length - lastStart)
            let continuation = NSMutableAttributedString(attributedString: text.attributedSubstring(from: remainder))
            let localRange = NSRange(location: 0, length: continuation.length)
            let breaks = Self.hardLineBreaks?.matches(in: continuation.string, range: localRange) ?? []
            var contentsEnd = 0
            source.getParagraphStart(nil, end: nil, contentsEnd: &contentsEnd,
                                     for: NSRange(location: lastStart, length: 0))
            let paragraphLength = max(0, min(contentsEnd - lastStart, breaks.first?.range.location ?? remainder.length))
            for match in breaks.reversed() {
                continuation.replaceCharacters(in: match.range, with: " ")
            }
            continuation.addAttribute(paragraphKey, value: rtl ? right : left,
                                      range: NSRange(location: 0, length: continuation.length))
            let line = CTLineCreateWithAttributedString(continuation as CFAttributedString)
            let fitted = truncation.map { Self.ellipsizedLine(line, text: continuation, width: width, placement: $0) }
            let paragraph = paragraph(at: lastStart, rtl: rtl)
            lines[lines.count - 1] = OverflowLine(fitted ?? line, top: last.top, width: width, rtl: rtl, paragraph: paragraph,
                                               paragraphLength: truncation == nil ? paragraphLength : nil)
        }
        let measuredWidth = min(width, ceil(lines.map(\.width).max() ?? 0))
        return (lines, NSSize(width: measuredWidth, height: lines.last?.bottom ?? 0))
    }

    private static func ellipsizedLine(
        _ line: CTLine, text: NSAttributedString, width: CGFloat, placement: CTLineTruncationType
    ) -> CTLine {
        let tokenText = NSAttributedString(string: "…", attributes: text.attributes(at: text.length - 1, effectiveRange: nil))
        let token = CTLineCreateWithAttributedString(tokenText as CFAttributedString)
        let lineWidth = CTLineGetTypographicBounds(line, nil, nil, nil)
        let tokenWidth = CTLineGetTypographicBounds(token, nil, nil, nil)
        var truncationWidth = Double(width)
        if lineWidth <= truncationWidth {
            // A hard break or the height limit can hide text even when it fits horizontally.
            let candidate = NSMutableAttributedString(attributedString: text)
            let insertion: Int
            switch placement {
            case .start: insertion = 0
            case .middle:
                insertion = (text.string as NSString).rangeOfComposedCharacterSequence(at: text.length / 2).location
            default: insertion = text.length
            }
            candidate.insert(tokenText, at: insertion)
            let withToken = CTLineCreateWithAttributedString(candidate as CFAttributedString)
            if CTLineGetTypographicBounds(withToken, nil, nil, nil) <= truncationWidth { return withToken }
            truncationWidth = lineWidth.nextDown
        }
        guard truncationWidth > tokenWidth else { return token }
        return CTLineCreateTruncatedLine(line, truncationWidth, placement, token) ?? token
    }

    private static func coreTextParagraph(rtl: Bool) -> CTParagraphStyle {
        var direction = rtl ? CTWritingDirection.rightToLeft : .leftToRight
        return withUnsafePointer(to: &direction) { value in
            var setting = CTParagraphStyleSetting(spec: .baseWritingDirection,
                                                   valueSize: MemoryLayout<CTWritingDirection>.size, value: value)
            return CTParagraphStyleCreate(&setting, 1)
        }
    }

    private static func glyphEdges(in line: CTLine, before end: Int) -> ClosedRange<CGFloat>? {
        (CTLineGetGlyphRuns(line) as! [CTRun]).reduce(nil as ClosedRange<CGFloat>?) { edges, run in
            guard let next = glyphEdges(in: run, before: end) else { return edges }
            guard let edges else { return next }
            return min(edges.lowerBound, next.lowerBound)...max(edges.upperBound, next.upperBound)
        }
    }

    private static func glyphEdges(in run: CTRun, before end: Int? = nil) -> ClosedRange<CGFloat>? {
        let range = CTRunGetStringRange(run)
        let count = CTRunGetGlyphCount(run)
        guard count > 0, end.map({ range.location < $0 }) ?? true else { return nil }
        let all = CFRange(location: 0, length: count)
        var positions = Array(repeating: CGPoint.zero, count: count)
        var advances = Array(repeating: CGSize.zero, count: count)
        CTRunGetPositions(run, all, &positions)
        CTRunGetAdvances(run, all, &advances)
        var indices: [Int] = []
        if let end, range.location + range.length > end {
            indices = Array(repeating: 0, count: count)
            CTRunGetStringIndices(run, all, &indices)
        }
        var left = CGFloat.infinity
        var right = -CGFloat.infinity
        for index in positions.indices {
            if let end, !indices.isEmpty, indices[index] >= end { continue }
            let x = positions[index].x
            let next = x + advances[index].width
            left = min(left, min(x, next))
            right = max(right, max(x, next))
        }
        return left <= right ? left...right : nil
    }

    struct OverflowLine {
        let line: CTLine
        let x: CGFloat
        let width: CGFloat
        let top: CGFloat
        let baseline: CGFloat
        let bottom: CGFloat

        init(_ line: CTLine, top: CGFloat, width availableWidth: CGFloat, rtl: Bool,
             paragraph: NSParagraphStyle? = nil, paragraphLength: Int? = nil) {
            self.line = line
            self.top = top
            var ascent: CGFloat = 0
            var descent: CGFloat = 0
            var leading: CGFloat = 0
            let advance = CGFloat(CTLineGetTypographicBounds(line, &ascent, &descent, &leading))
            width = max(0, advance - CGFloat(CTLineGetTrailingWhitespaceWidth(line)))
            let natural = ascent + descent + max(0, leading)
            let multiple = paragraph?.lineHeightMultiple ?? 0
            var height = natural * (multiple > 0 ? multiple : 1)
            if let paragraph {
                height = max(height, paragraph.minimumLineHeight)
                if paragraph.maximumLineHeight > 0 { height = min(height, paragraph.maximumLineHeight) }
            }
            baseline = top + ascent + (height - natural) / 2
            bottom = top + ceil(height)
            if width > availableWidth {
                let edges = paragraphLength.flatMap { JadixLabel.glyphEdges(in: line, before: $0) }
                x = rtl ? availableWidth - (edges?.upperBound ?? width) : -(edges?.lowerBound ?? 0)
            } else {
                let flush: CGFloat
                switch paragraph?.alignment {
                case .center: flush = 0.5
                case .right: flush = 1
                case .left: flush = 0
                default: flush = rtl ? 1 : 0
                }
                x = CGFloat(CTLineGetPenOffsetForFlush(line, flush, Double(availableWidth)))
            }
        }

        func draw(in context: CGContext, at origin: CGPoint) {
            // Paint inline backgrounds and authored decorations around Core Text glyphs.
            for run in CTLineGetGlyphRuns(line) as! [CTRun] {
                let attributes = CTRunGetAttributes(run) as NSDictionary
                guard let color = attributes[NSAttributedString.Key.backgroundColor.rawValue] as? NSColor,
                      let rect = decorationRect(for: run, at: origin, strike: false) else { continue }
                context.setFillColor(color.cgColor)
                context.fill(rect)
            }
            context.textPosition = origin
            context.saveGState()
            if let run = (CTLineGetGlyphRuns(line) as! [CTRun]).first,
               let shadow = (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.shadow.rawValue] as? NSShadow {
                context.setShadow(offset: shadow.shadowOffset, blur: shadow.shadowBlurRadius,
                                  color: (shadow.shadowColor as? NSColor)?.cgColor)
            }
            CTLineDraw(line, context)
            context.restoreGState()
            for run in CTLineGetGlyphRuns(line) as! [CTRun] {
                let attributes = CTRunGetAttributes(run) as NSDictionary
                guard let edges = JadixLabel.glyphEdges(in: run) else { continue }
                let appearance = attributes[NSAttributedString.Key.jadixTextAppearance.rawValue] as? [String: Any] ?? [:]
                let color = JadixVisualStyle.color(appearance["decoration_color"])
                    ?? attributes[NSAttributedString.Key.foregroundColor.rawValue] as? NSColor ?? .labelColor
                var ascent: CGFloat = 0, descent: CGFloat = 0
                _ = CTRunGetTypographicBounds(run, CFRange(location: 0, length: 0), &ascent, &descent, nil)
                let thickness = CGFloat(appearance["decoration_thickness"] as? Double ?? Double(max(1, ascent / 14)))
                let style = appearance["decoration_style"] as? String ?? "solid"
                func stroke(_ y: CGFloat) {
                    JadixTextAppearance.drawDecoration(context, from: origin.x + edges.lowerBound,
                        width: edges.upperBound - edges.lowerBound, y: origin.y + y,
                        thickness: thickness, style: style, color: color)
                }
                if attributes[NSAttributedString.Key.jadixUnderline.rawValue] as? Bool == true {
                    stroke(-CGFloat(appearance["underline_offset"] as? Double ?? Double(max(1, descent / 2))))
                }
                if (attributes[NSAttributedString.Key.strikethroughStyle.rawValue] as? Int ?? 0) != 0 { stroke(ascent * 0.35) }
                if appearance["overline"] as? Bool == true { stroke(ascent) }
            }
        }

        private func decorationRect(for run: CTRun, at origin: CGPoint, strike: Bool) -> CGRect? {
            guard let edges = JadixLabel.glyphEdges(in: run), edges.upperBound > edges.lowerBound else { return nil }
            var position = CGPoint.zero
            CTRunGetPositions(run, CFRange(location: 0, length: 1), &position)
            var ascent: CGFloat = 0
            var descent: CGFloat = 0
            _ = CTRunGetTypographicBounds(run, CFRange(location: 0, length: 0), &ascent, &descent, nil)
            return CGRect(x: origin.x + edges.lowerBound,
                          y: origin.y + position.y + (strike ? ascent * 0.35 : -descent),
                          width: edges.upperBound - edges.lowerBound,
                          height: strike ? max(1, ascent / 14) : ascent + descent)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let layout = overflowLayout(width: bounds.width, height: bounds.height),
              let context = NSGraphicsContext.current?.cgContext else {
            super.draw(dirtyRect)
            return
        }
        context.saveGState()
        defer { context.restoreGState() }
        context.clip(to: bounds)
        context.translateBy(x: bounds.minX, y: isFlipped ? bounds.minY : bounds.maxY)
        if isFlipped { context.scaleBy(x: 1, y: -1) }
        context.textMatrix = .identity
        let offset = textAppearance?.verticalOffset(available: bounds.height, used: layout.size.height) ?? 0
        for line in layout.lines {
            line.draw(in: context, at: CGPoint(x: line.x, y: -line.baseline - offset))
        }
    }
}

private final class JadixTextView: NSTextView {
    var placeholder = "" { didSet { needsDisplay = true } }
    var placeholderColor: NSColor = .placeholderTextColor { didSet { needsDisplay = true } }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty, !placeholder.isEmpty else { return }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize),
            .foregroundColor: placeholderColor,
            .paragraphStyle: defaultParagraphStyle ?? NSParagraphStyle.default,
        ]
        placeholder.draw(at: NSPoint(x: textContainerInset.width, y: textContainerInset.height), withAttributes: attributes)
    }
}

private final class JadixScrollView: NSScrollView {
    var lastScrollTarget: [Double]?
    var onWheel: ((CGFloat, CGFloat) -> Void)?
    var onZoom: (() -> Void)?
    private var zoomPolicy: NSDictionary?
    private var gestureZoom = false
    var lastReportedScale: CGFloat = 1
    var lastReportedViewport = NSSize.zero
    var lastReportedOffset = NSPoint.zero
    var offsetReportPending = false

    private var scrollTimer: Timer?
    private var animatedOffset: NSPoint?

    func cancelScrollAnimation() {
        scrollTimer?.invalidate(); scrollTimer = nil; animatedOffset = nil
    }

    override func reflectScrolledClipView(_ clipView: NSClipView) {
        // Scroller drags and native keyboard scrolling supersede a pending target.
        if scrollTimer != nil, let expected = animatedOffset,
           clipView === contentView, clipView.bounds.origin != expected {
            cancelScrollAnimation()
        }
        super.reflectScrolledClipView(clipView)
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil { cancelScrollAnimation() }
    }

    deinit { scrollTimer?.invalidate() }

    func scroll(to destination: NSPoint, smoothly: Bool) {
        cancelScrollAnimation()
        var bounds = contentView.bounds; bounds.origin = destination
        let target = contentView.constrainBoundsRect(bounds).origin
        let start = contentView.bounds.origin
        guard smoothly, window != nil, !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion,
              hypot(target.x - start.x, target.y - start.y) > 0.5 else {
            contentView.scroll(to: target); reflectScrolledClipView(contentView)
            return
        }
        let began = ProcessInfo.processInfo.systemUptime
        let timer = Timer(timeInterval: 1.0 / 60, repeats: true) { [weak self] _ in
            guard let self, self.window != nil else { self?.cancelScrollAnimation(); return }
            let elapsed = ProcessInfo.processInfo.systemUptime - began
            let progress = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 1 : min(1, elapsed / 0.22)
            let eased = 1 - pow(1 - progress, 3)
            var rect = self.contentView.bounds
            rect.origin = NSPoint(x: start.x + (target.x - start.x) * eased,
                                  y: start.y + (target.y - start.y) * eased)
            let offset = self.contentView.constrainBoundsRect(rect).origin
            self.animatedOffset = offset
            self.contentView.scroll(to: offset)
            self.reflectScrolledClipView(self.contentView)
            if progress >= 1 { self.cancelScrollAnimation() }
        }
        animatedOffset = start
        scrollTimer = timer
        RunLoop.main.add(timer, forMode: .common)
    }

    override func scrollWheel(with event: NSEvent) {
        cancelScrollAnimation()
        if gestureZoom, event.modifierFlags.contains(.control) {
            zoom(to: magnification * exp(-event.scrollingDeltaY * 0.01), anchor: anchor(for: event))
            return
        }
        onWheel?(event.scrollingDeltaX, event.scrollingDeltaY)
        super.scrollWheel(with: event)
    }

    override func magnify(with event: NSEvent) {
        cancelScrollAnimation()
        guard gestureZoom else { super.magnify(with: event); return }
        zoom(to: magnification * (1 + event.magnification), anchor: anchor(for: event))
    }

    private func anchor(for event: NSEvent) -> NSPoint {
        let point = contentView.convert(event.locationInWindow, from: nil)
        return NSPoint(x: (point.x - contentView.bounds.minX) * magnification,
                       y: (point.y - contentView.bounds.minY) * magnification)
    }

    func configureZoom(_ value: [String: Any]?) {
        let next = value.map { NSDictionary(dictionary: $0) }
        guard zoomPolicy != next else { return }
        let reset = zoomPolicy == nil || zoomPolicy?["initial"] as? Double != next?["initial"] as? Double
        zoomPolicy = next
        gestureZoom = value != nil && (value?["gestures"] as? Bool ?? true)
        allowsMagnification = false // All changes use the same anchor/coordinate contract.
        if let value {
            let low = CGFloat(value["min"] as? Double ?? 0.25)
            let high = CGFloat(value["max"] as? Double ?? 4)
            minMagnification = min(low, minMagnification)
            maxMagnification = max(high, maxMagnification)
            minMagnification = low; maxMagnification = high
            zoom(to: reset ? CGFloat(value["initial"] as? Double ?? 1) : magnification, anchor: .zero)
        } else {
            minMagnification = min(1, minMagnification); maxMagnification = max(1, maxMagnification)
            magnification = 1
        }
    }

    func zoom(to scale: CGFloat, anchor: NSPoint) {
        guard zoomPolicy != nil, scale.isFinite, scale > 0 else { return }
        let old = magnification
        let scale = min(maxMagnification, max(minMagnification, scale))
        let offset = contentView.bounds.origin
        let x = min(contentSize.width, max(0, anchor.x))
        let y = min(contentSize.height, max(0, anchor.y))
        magnification = scale
        var rect = contentView.bounds
        rect.origin = NSPoint(x: offset.x + x / old - x / scale, y: offset.y + y / old - y / scale)
        contentView.scroll(to: contentView.constrainBoundsRect(rect).origin)
        reflectScrolledClipView(contentView)
        onZoom?()
    }
}

public final class JadixNativeView: NSView, NSTextFieldDelegate, NSTextViewDelegate {
    private var webResourceReplies: [UInt64: (Result<JadixWebResourceResponse, Error>) -> Void] = [:]
    var hasVisibleOverlay: Bool {
        subviews.contains { view in
            guard let overlay = view as? JadixOverlayView else { return false }
            return overlay.isOpen && !overlay.isHidden
        }
    }
    public enum FileSelection: Equatable {
        case files
        case folders
        case filesAndFolders
    }

    private struct FileRequest: Decodable {
        enum Operation: String, Decodable {
            case pickPaths = "pick_paths"
            case pickSavePath = "pick_save_path"
            case open, reveal, trash
            case openURL = "open_url"
        }

        struct Options: Decodable {
            let contentTypes: [String]
            let directory: String?
            let title: String?
            let prompt: String?
        }

        let operation: Operation
        let url: String?
        let options: Options?
        let selection: String?
        let multiple: Bool?
        let fileName: String?
        let path: String?
    }

    private struct FileReply: Encodable {
        var Ok: [String]?
        var Err: String?
    }

    public private(set) var root: UInt64 = 0
    private var mountingWindowRequests: [(UInt64, JadixWindowRequest)]?
    private var configuredRemoteImagePolicy = JadixRemoteImagePolicy()
    private var embeddedFactories: [String: JadixEmbeddedViewFactory] = [:]
    private var remoteImages: AppleRemoteImages?
    private let callbackContext = AppKitCallbackContext()
    private var environment: UInt64 = 1
    private var pendingImageLayoutRoot: UInt64?
    private var appearanceRevision: UInt64 = 0
    private var appliedAppearanceRevision: UInt64 = 0
    private var publishing = false
    private var applyingClippedText = false
    private var views: [String: NSView] = [:]
    private var viewAppearances: [String: JadixViewAppearance] = [:]
    private var recycledViews: [String: [NSView]] = [:]
    private var preboundViews: [String: (key: String, view: NSView)] = [:]
    private var recycledViewCount = 0
    private var nodeIDsByView: [ObjectIdentifier: String] = [:]
    private var descriptors: [String: [String: Any]] = [:]
    private var deferredEditorAppearances = Set<String>()
    private var windowEventMonitor: Any?
    private weak var outsideDismissedPopover: JadixPopoverView?
    private var openPopoverIDs: [String] = []
    private var popoverPositions: [String: JadixPopoverEvent.Position] = [:]
    private var popoverEventGeneration = UUID()
    private var accessibilityActiveDescendants: [String: String] = [:]
    private var accessibilityGridRows: [String: [Int: JadixAccessibilityRow]] = [:]
    private var accessibilityGridColumns: [String: [Int: JadixAccessibilityColumn]] = [:]
    private var nodeOrder: [String] = []
    private var revisions: [String: UInt64] = [:]
    private var scrollObservers: [String: NSObjectProtocol] = [:]
    private var focusedID: String?
    private weak var previousFocus: NSView?
    private weak var dialogFocusWindow: NSWindow?
    private weak var observedWindow: NSWindow?
    private weak var activeFilePanel: NSSavePanel?
    private var systemAlerts: [UInt64: JadixSystemAlert] = [:]
    private var systemShare: JadixSystemShare?
    private var activeContextMenu: NSMenu?
    private static var originalMenuBar: JadixMenuBar?
    private static var installedMenuBar: NSMenu?
    private static weak var menuBarOwner: JadixNativeView?
    private var windowMenuBar: JadixMenuBar?
    private var windowMenuRevision: UInt64 = 0
    private var activeDialogID: String?
    private var dialogAnnouncementContent: [String: String] = [:]
    private var savedKeyViewLinks: [ObjectIdentifier: (NSView, NSView?)] = [:]
    private var keyViewLoopDialogID: String?
    private let speechService = JadixSpeechService()
    private var fetchService = JadixFetchService()
    private let desktopServices = JadixDesktopServices()
    private var audioService = JadixAudioService()
    private let statusItem = JadixStatusItem()
    private var webPump: Timer?
    private var performanceLink: CADisplayLink?
    private let performancePanel = JadixPerformancePanel(frame: .zero)
    private var performanceWork: CFTimeInterval = 0
    private var lastPerformanceUpdate: CFTimeInterval = 0
    public var performanceVisible = ProcessInfo.processInfo.environment["JADIX_FPS"] == "1" {
        didSet { updatePerformancePanel() }
    }
    private func updatePerformancePanel() {
        performancePanel.isHidden = !performanceVisible
        if performanceVisible && performancePanel.superview == nil {
            addSubview(performancePanel, positioned: .above, relativeTo: nil)
            performancePanel.resized = { [weak self] in self?.positionPerformancePanel() }
        }
        positionPerformancePanel()
    }
    private func positionPerformancePanel() {
        performancePanel.setFrameOrigin(NSPoint(x: max(0, bounds.width - performancePanel.frame.width - 8), y: 8))
    }
    func performanceTick(_ link: CADisplayLink) {
        guard root != 0 else { return }
        let duration = link.targetTimestamp - link.timestamp
        let work = performanceWork; performanceWork = 0
        jadix_native_display_tick(root, duration > 0 ? 1 / duration : 0, min(60, max(0, work)), work > 0)
        let now = CACurrentMediaTime()
        guard (performanceVisible || ProcessInfo.processInfo.environment["JADIX_PERF_TRACE"] == "1"), now - lastPerformanceUpdate >= 0.25 else { return }
        lastPerformanceUpdate = now
        guard let pointer = jadix_native_performance(root) else { return }
        defer { jadix_native_string_free(pointer) }
        if let data = String(cString: pointer).data(using: .utf8),
           let sample = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            performancePanel.sample = sample
        }
    }
    private var environmentObservers: [NSObjectProtocol] = []
    private var windowStateObservers: [NSObjectProtocol] = []
    private var normalWindowFrame: NSRect?
    private var zoomingWindow = false
    private var pendingWindowStateRoot: UInt64?
    private var lastSize = NSSize.zero
    private var handlingWindowRequest = false
    private var closeAfterWindowRequest = false
    public var onError: ((Error) -> Void)?
    public var onStateChange: ((String) -> Void)?
    public var onWindowStateChange: (([String: Any]?) -> Void)?
    public var onWindowRequest: ((JadixWindowRequest) throws -> Void)?
    /// Return the key, titled window in the host's window group, when available.
    public var preferredSheetWindow: (() -> NSWindow?)?
    public var onPopoverEvent: ((String, JadixPopoverEvent) -> Void)?
    public var onOpenURL: ((URL) -> Bool)?
    public var handoffEnabled = false {
        didSet { refreshHandoff() }
    }
    private let handoff = JadixHandoff()

    public override var isFlipped: Bool { true }

    public static var declaresHandoff: Bool { JadixHandoff.declared }

    /// Use for normal URL-open events and URLs delivered by Handoff.
    @discardableResult
    public func openURL(_ url: URL, source: String = "hot_start") -> Bool {
        guard root != 0, Self.acceptsURL(url), ["cold_start", "hot_start", "handoff"].contains(source) else { return false }
        if onOpenURL?(url) == true { return true }
        do {
            let data = try JSONSerialization.data(withJSONObject: ["url": url.absoluteString, "source": source])
            guard let json = String(data: data, encoding: .utf8) else { return false }
            _ = try reply(json.withCString { jadix_native_deep_link(root, $0, false) }); return true
        } catch { onError?(error); return false }
    }

    static func acceptsURL(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased(), !["file", "http", "https"].contains(scheme),
              let types = Bundle.main.object(forInfoDictionaryKey: "CFBundleURLTypes") as? [[String: Any]]
        else { return false }
        return types.contains { type in
            let schemes = type["CFBundleURLSchemes"] as? [String] ?? []
            return schemes.contains { $0.lowercased() == scheme }
        }
    }

    @discardableResult
    public func continueUserActivity(_ activity: NSUserActivity) -> Bool {
        guard handoffEnabled, let url = Self.continuedURL(activity) else { return false }
        return openURL(url, source: "handoff")
    }

    static func continuedURL(_ activity: NSUserActivity) -> URL? { JadixHandoff.continuedURL(activity) }

    public func setAppAddress(_ address: String?) throws {
        guard root != 0 else { throw AppKitError.invalid("unmounted native root") }
        if let address {
            guard !address.utf8.contains(0) else { throw AppKitError.invalid("invalid app address") }
            _ = try reply(address.withCString { jadix_native_set_app_address(root, $0) })
        } else {
            _ = try reply(jadix_native_set_app_address(root, nil))
        }
    }

    private func applyAppAddress(_ address: String?) {
        handoff.setAddress(address); refreshHandoff()
    }

    private func refreshHandoff() {
        handoff.refresh(enabled: handoffEnabled, active: root != 0 && window?.isKeyWindow == true)
    }

    public func restrictRemoteImages(to policy: JadixRemoteImagePolicy) throws {
        guard root == 0 else { throw AppKitError.invalid("image policy must be set before mounting") }
        configuredRemoteImagePolicy = policy
    }

    /// Stable window/application scope used to replay notification clicks after launch.
    public func setNotificationScope(_ scope: String) throws {
        guard root == 0, !scope.isEmpty, scope.utf8.count <= 64,
              scope.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0)
                  || (97...122).contains($0) || [45, 46, 95].contains($0) }) else {
            throw AppKitError.invalid("invalid or already mounted notification scope")
        }
        desktopServices.scope = scope
    }

    public func registerEmbeddedView(_ kind: String, factory: @escaping JadixEmbeddedViewFactory) throws {
        guard root == 0, embeddedFactories.count < 64, embeddedFactories[kind] == nil,
              !kind.isEmpty, kind.utf8.count <= 128,
              kind.utf8.allSatisfy({ (65...90).contains($0) || (97...122).contains($0)
                  || (48...57).contains($0) || [45, 46, 95].contains($0) }) else {
            throw AppKitError.invalid("invalid, duplicate or already mounted native view factory")
        }
        embeddedFactories[kind] = factory
    }

    public typealias RootFactory = (
        UnsafeMutableRawPointer, JadixNativeMeasure, JadixNativePublish, JadixNativeClear,
        JadixNativeFileRequest, JadixNativeWindowRequest, JadixNativeAlertRequest,
        JadixNativeShareRequest, Float, Float
    ) -> UInt64

    public func mount(factory: RootFactory? = nil) throws {
        precondition(Thread.isMainThread)
        guard root == 0, mountingWindowRequests == nil else {
            throw AppKitError.invalid("root already mounted")
        }
        JadixApplicationUpdater.shared.start()
        wantsLayer = true
        callbackContext.host = self
        mountingWindowRequests = []
        let create: RootFactory = factory ?? { context, measure, publish, clear, file, window, alert, share, width, height in
            jadix_native_create_with_services(context, measure, publish, clear, file, window, alert, share, width, height)
        }
        root = create(
            Unmanaged.passUnretained(callbackContext).toOpaque(), measureAppKit, publishAppKit,
            clearAppKit, requestAppKitFile, requestAppKitWindow, requestAppKitAlert, requestAppKitShare,
            Float(bounds.width), Float(bounds.height)
        )
        let windowRequests = mountingWindowRequests ?? []
        mountingWindowRequests = nil
        guard root != 0 else {
            callbackContext.host = nil
            clear()
            throw AppKitError.invalid("native mount failed")
        }
        let entries = Bundle.main.object(forInfoDictionaryKey: "CFBundleURLTypes") as? [[String: Any]] ?? []
        let schemes = Array(Set(entries.flatMap { $0["CFBundleURLSchemes"] as? [String] ?? [] }.map { $0.lowercased() }))
        do {
            let data = try JSONSerialization.data(withJSONObject: ["schemes": schemes, "single_instance": Bundle.main.object(forInfoDictionaryKey: "LSMultipleInstancesProhibited") as? Bool ?? true, "queue_limit": 64])
            let json = String(decoding: data, as: UTF8.self)
            _ = try reply(json.withCString { jadix_native_deep_link(root, $0, true) })
        } catch { close(); throw error }
        let serviceRoot = root
        desktopServices.authAnchor = { [weak self] in self?.window }
        desktopServices.output = { [weak self] id, event in
            DispatchQueue.main.async { [weak self] in
                guard let self, self.root == serviceRoot,
                      let data = try? JSONSerialization.data(withJSONObject: event), data.count <= 32 * 1024,
                      let json = String(data: data, encoding: .utf8) else { return }
                json.withCString { _ = jadix_native_desktop_service_event(serviceRoot, id, $0) }
            }
        }
        desktopServices.start()
        for (id, request) in windowRequests {
            enqueueWindowRequest(id, request: request, mountedRoot: root)
        }
        _ = jadix_native_enable_context_menus(root, requestAppKitContextMenu)
        lastSize = bounds.size
        let workspaceCenter = NSWorkspace.shared.notificationCenter
        environmentObservers.append(workspaceCenter.addObserver(
            forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification,
            object: NSWorkspace.shared,
            queue: .main
        ) { [weak self] _ in self?.invalidateEnvironment(refreshAppearance: true) })
        environmentObservers.append(NotificationCenter.default.addObserver(
            forName: NSFontCollection.didChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in self?.invalidateEnvironment(refreshAppearance: true) })
        observeWindowState()
        for name in [NSLocale.currentLocaleDidChangeNotification, NSNotification.Name.NSSystemTimeZoneDidChange,
                     UserDefaults.didChangeNotification, NSApplication.didBecomeActiveNotification,
                     NSApplication.didResignActiveNotification, NSApplication.didHideNotification,
                     NSApplication.didUnhideNotification, .jadixFontsChanged] {
            environmentObservers.append(NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) {
                [weak self] _ in self?.invalidateEnvironment(refreshAppearance: true)
            })
        }
        invalidateEnvironment(refreshAppearance: true)
        updatePerformancePanel()
        let target = JadixFrameTarget(self)
        performanceLink = displayLink(target: target, selector: #selector(JadixFrameTarget.tick(_:)))
        performanceLink?.add(to: .main, forMode: .common)
        webPump = Timer.scheduledTimer(withTimeInterval: 0.025, repeats: true) { [weak self] _ in
            guard let self, self.root != 0 else { return }
            jadix_native_web_poll(self.root)
        }
    }

    public func close() {
        precondition(Thread.isMainThread)
        if handlingWindowRequest {
            // A close request may target its own root. Reply before disposing it.
            closeAfterWindowRequest = true
            return
        }
        onStateChange = nil
        onWindowStateChange = nil
        let previous = root
        root = 0
        cancelSystemAlerts()
        systemAlerts.removeAll()
        cancelFilePanel()
        callbackContext.host = nil
        if previous != 0 { jadix_native_dispose(previous) }
        clear()
    }

    public func state() throws -> String {
        precondition(Thread.isMainThread)
        guard root != 0 else { throw AppKitError.invalid("unmounted native root") }
        return try reply(jadix_native_state(root))
    }

    public func restore(_ json: String) throws {
        precondition(Thread.isMainThread)
        guard root != 0 else { throw AppKitError.invalid("unmounted native root") }
        _ = try json.withCString { try reply(jadix_native_restore(root, $0)) }
    }

    fileprivate func scheduleWindowRequest(_ requestID: UInt64, json: String) -> Bool {
        guard onWindowRequest != nil,
              let request = try? JSONDecoder().decode(JadixWindowRequest.self, from: Data(json.utf8)) else {
            return false
        }
        if mountingWindowRequests != nil {
            mountingWindowRequests?.append((requestID, request))
            return true
        }
        guard root != 0 else { return false }
        enqueueWindowRequest(requestID, request: request, mountedRoot: root)
        return true
    }

    private func enqueueWindowRequest(_ requestID: UInt64, request: JadixWindowRequest, mountedRoot: UInt64) {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.root == mountedRoot else { return }
            self.handlingWindowRequest = true
            defer {
                self.handlingWindowRequest = false
                if self.closeAfterWindowRequest {
                    self.closeAfterWindowRequest = false
                    self.close()
                }
            }
            let response: String
            do {
                guard let handle = self.onWindowRequest else {
                    throw AppKitError.invalid("native window requests are unavailable")
                }
                try handle(request)
                response = "{\"Ok\":null}"
            } catch {
                guard let data = try? JSONEncoder().encode(["Err": error.localizedDescription]),
                      let json = String(data: data, encoding: .utf8) else { return }
                response = json
            }
            _ = response.withCString { jadix_native_window_result(mountedRoot, requestID, $0) }
        }
    }

    fileprivate func scheduleAlertRequest(_ id: UInt64, json: String) -> Bool {
        let mountedRoot = root
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard mountedRoot != 0,
              let request = try? decoder.decode(JadixAlertRequest.self, from: Data(json.utf8)),
              request.operation == .close || request.alert != nil else {
            return false
        }
        DispatchQueue.main.async { [weak self] in
            guard let self, self.root == mountedRoot else { return }
            switch request.operation {
            case .close:
                self.systemAlerts[id]?.cancel()
            case .show:
                guard let options = request.alert else { return }
                let pending = JadixSystemAlert(id: id, options: options)
                self.systemAlerts[id] = pending
                let owner = self.preferredSheetWindow?() ?? self.window
                let appearance = owner?.effectiveAppearance ?? self.effectiveAppearance
                pending.present(in: owner, appearance: appearance) { [weak self, weak pending] response in
                    guard let self, let pending, self.systemAlerts[id] === pending else { return }
                    self.systemAlerts.removeValue(forKey: id)
                    guard self.root == mountedRoot else { return }
                    let index = response.rawValue - NSApplication.ModalResponse.alertFirstButtonReturn.rawValue
                    let button: Int? = pending.alert.buttons.indices.contains(index) ? index : nil
                    let result: [String: Any] = [
                        "button": button.map { $0 as Any } ?? NSNull(),
                        "input": pending.inputValue.map { $0 as Any } ?? NSNull()
                    ]
                    DispatchQueue.main.async { [weak self] in
                        guard let self, self.root == mountedRoot else { return }
                        self.sendAlertResult(id, ["Ok": result])
                    }
                }
            }
        }
        return true
    }

    private func cancelSystemAlerts(attachedOnly: Bool = false) {
        // End queued sheets before ending the visible sheet.
        let alerts = systemAlerts.values.sorted { $0.id > $1.id }
        for alert in alerts where !attachedOnly || alert.owner != nil { alert.cancel() }
    }

    private func sendAlertResult(_ id: UInt64, _ value: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: value) else { return }
        let json = String(decoding: data, as: UTF8.self)
        _ = json.withCString { jadix_native_alert_result(root, id, $0) }
    }

    fileprivate func scheduleContextMenu(_ id: UInt64, json: String) -> Bool {
        let mountedRoot = root
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard mountedRoot != 0,
              let request = try? decoder.decode(JadixContextMenuRequest.self, from: Data(json.utf8)),
              request.anchorRoot == String(mountedRoot) else { return false }
        RunLoop.main.perform(inModes: [.default, .modalPanel]) { [weak self] in
            MainActor.assumeIsolated {
                guard let self, self.root == mountedRoot else { return }
                let result = self.presentContextMenu(request)
                guard self.root == mountedRoot,
                      let data = try? JSONSerialization.data(withJSONObject: result) else { return }
                let json = String(decoding: data, as: UTF8.self)
                _ = json.withCString { jadix_native_context_menu_result(mountedRoot, id, $0) }
            }
        }
        CFRunLoopWakeUp(CFRunLoopGetMain())
        return true
    }

    private func presentContextMenu(_ request: JadixContextMenuRequest) -> [String: Any] {
        guard activeContextMenu == nil else { return ["Err": "An AppKit context menu is already open"] }
        guard let anchor = views[request.anchor], let owner = panelSheetOwner(anchor.window),
              !anchor.isHiddenOrHasHiddenAncestor, !anchor.visibleRect.isEmpty else {
            return ["Err": "AppKit context menu requires a visible anchor"]
        }
        let selection = JadixMenuTarget(remoteImages: imageLoader(), scale: owner.backingScaleFactor)
        let menu = selection.build(request.items)
        activeContextMenu = menu
        defer {
            selection.cancelIcons()
            if activeContextMenu === menu { activeContextMenu = nil }
        }
        let rect = anchor.visibleRect
        let y = anchor.isFlipped ? rect.maxY : rect.minY
        menu.popUp(positioning: nil, at: NSPoint(x: rect.minX, y: y), in: anchor)
        guard views[request.anchor] === anchor, panelSheetOwner(anchor.window) === owner else {
            return ["Ok": NSNull()]
        }
        if let command = selection.command { return ["Ok": command] }
        return ["Ok": NSNull()]
    }

    private func applyWindowMenu(_ request: JadixMenuBarRequest) {
        windowMenuBar?.target?.cancelIcons()
        windowMenuRevision = request.revision
        if request.items.isEmpty {
            windowMenuBar = nil
        } else {
            let scale = window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 1
            let target = JadixMenuTarget(remoteImages: imageLoader(), scale: scale)
            let mountedRoot = root
            let revision = request.revision
            target.dispatch = { [weak self] id in
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.root == mountedRoot,
                          self.window?.isKeyWindow == true, Self.menuBarOwner === self,
                          self.windowMenuRevision == revision else { return }
                    _ = id.withCString { jadix_native_menu_command(mountedRoot, revision, $0) }
                }
            }
            windowMenuBar = JadixMenuBar(request: request, target: target)
        }
        refreshWindowMenu()
    }

    private func refreshWindowMenu() {
        let application = NSApplication.shared
        if let installed = Self.installedMenuBar, application.mainMenu !== installed {
            Self.installedMenuBar = nil
            Self.originalMenuBar = nil
            Self.menuBarOwner = nil
        }
        if root != 0, window?.isKeyWindow == true, let bar = windowMenuBar {
            if Self.installedMenuBar === bar.main { return }
            if Self.originalMenuBar == nil {
                Self.originalMenuBar = JadixMenuBar(application: application)
            }
            Self.menuBarOwner = self
            Self.installedMenuBar = bar.main
            bar.install(in: application)
        } else if Self.menuBarOwner === self {
            let original = Self.originalMenuBar
            Self.menuBarOwner = nil
            Self.installedMenuBar = nil
            Self.originalMenuBar = nil
            original?.install(in: application)
        }
    }

    fileprivate func scheduleShareRequest(_ id: UInt64, json: String) -> Bool {
        let mountedRoot = root
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard mountedRoot != 0,
              let request = try? decoder.decode(JadixShareRequest.self, from: Data(json.utf8)),
              request.anchorRoot.map({ $0 == String(mountedRoot) }) ?? true else { return false }
        RunLoop.main.perform(inModes: [.default, .modalPanel]) { [weak self] in
            MainActor.assumeIsolated {
                guard let self, self.root == mountedRoot else { return }
                self.presentShare(request, id: id, mountedRoot: mountedRoot)
            }
        }
        CFRunLoopWakeUp(CFRunLoopGetMain())
        return true
    }

    private func presentShare(_ request: JadixShareRequest, id: UInt64, mountedRoot: UInt64) {
        systemShare?.cancel(replaced: true)
        let respond: ([String: Any]) -> Void = { [weak self] result in
            DispatchQueue.main.async { [weak self] in
                guard let self, self.root == mountedRoot,
                      let data = try? JSONSerialization.data(withJSONObject: result) else { return }
                if self.systemShare?.id == id { self.systemShare = nil }
                let json = String(decoding: data, as: UTF8.self)
                _ = json.withCString { jadix_native_share_result(mountedRoot, id, $0) }
            }
        }
        var items: [Any] = []
        if let text = request.text { items.append(text) }
        if let value = request.url {
            guard let url = URL(string: value), let scheme = url.scheme, !scheme.isEmpty,
                  !url.isFileURL else {
                respond(["Err": "AppKit share requires an absolute non-file URL"])
                return
            }
            items.append(url)
        }
        items.append(contentsOf: request.files.map { URL(fileURLWithPath: $0) as Any })
        let anchor = request.anchor.flatMap { views[$0] }.flatMap { view in
            panelSheetOwner(view.window) != nil && !view.isHiddenOrHasHiddenAncestor ? view : nil
        }
        let view = anchor ?? self
        guard panelSheetOwner(view.window) != nil else {
            respond(["Err": "AppKit share requires a visible window"])
            return
        }
        guard !items.isEmpty else {
            respond(["Err": "nothing to share"])
            return
        }
        let rect = anchor?.bounds ?? NSRect(x: bounds.midX, y: bounds.midY, width: 1, height: 1)
        let share = JadixSystemShare(id: id, items: items, subject: request.subject, completion: respond)
        systemShare = share
        share.show(relativeTo: rect, of: view)
    }

    fileprivate func scheduleFileRequest(_ id: UInt64, json: String) -> Bool {
        let mountedRoot = root
        let owner = window
        guard mountedRoot != 0 else { return false }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let request = try? decoder.decode(FileRequest.self, from: Data(json.utf8)) else { return false }
        Task { @MainActor [weak self] in
            guard let self, self.root == mountedRoot else { return }
            let response: FileReply
            do {
                var paths: [String] = []
                if (request.operation != .pickPaths && request.operation != .pickSavePath)
                    || self.window === owner {
                    paths = try await self.performFileRequest(request).map(\.path)
                }
                response = FileReply(Ok: paths)
            } catch {
                response = FileReply(Err: error.localizedDescription)
            }
            guard self.root == mountedRoot,
                  let data = try? JSONEncoder().encode(response),
                  let json = String(data: data, encoding: .utf8) else { return }
            _ = json.withCString { jadix_native_file_result(mountedRoot, id, $0) }
        }
        return true
    }

    private func performFileRequest(_ request: FileRequest) async throws -> [URL] {
        switch request.operation {
        case .openURL:
            guard let value = request.url, value.utf8.count <= 16384,
                  !value.unicodeScalars.contains(where: { CharacterSet.whitespacesAndNewlines.union(.controlCharacters).contains($0) }),
                  let url = URL(string: value), let scheme = url.scheme?.lowercased(), scheme.count > 1,
                  !["javascript", "vbscript", "data", "blob", "file", "about", "asset"].contains(scheme),
                  !["http", "https"].contains(scheme) || url.host?.isEmpty == false else {
                throw AppKitError.invalid("invalid external URL")
            }
            guard NSWorkspace.shared.open(url) else { throw AppKitError.invalid("could not open URL") }
            return []
        case .pickPaths, .pickSavePath:
            guard let options = request.options else { throw AppKitError.invalid("missing file panel options") }
            let types = try options.contentTypes.map { identifier in
                guard let type = UTType(identifier) else {
                    throw AppKitError.invalid("invalid content type: \(identifier)")
                }
                return type
            }
            let directory = options.directory.map { URL(fileURLWithPath: $0, isDirectory: true) }
            if request.operation == .pickSavePath {
                let url = try await pickSavePath(fileName: request.fileName, types: types,
                                                  directory: directory, title: options.title, prompt: options.prompt)
                return url.map { [$0] } ?? []
            }
            let selection: FileSelection
            switch request.selection {
            case "files": selection = .files
            case "folders": selection = .folders
            case "files_and_folders": selection = .filesAndFolders
            default: throw AppKitError.invalid("invalid file panel selection")
            }
            return try await pickPaths(selection, multiple: request.multiple ?? false, types: types,
                                       directory: directory, title: options.title, prompt: options.prompt)
        case .open, .reveal, .trash:
            guard let path = request.path, !path.isEmpty else {
                throw AppKitError.invalid("missing file path")
            }
            let url = URL(fileURLWithPath: path)
            switch request.operation {
            case .open:
                guard NSWorkspace.shared.open(url) else { throw AppKitError.invalid("could not open \(path)") }
            case .reveal:
                guard FileManager.default.fileExists(atPath: url.path) else {
                    throw AppKitError.invalid("file does not exist: \(path)")
                }
                NSWorkspace.shared.activateFileViewerSelecting([url])
            case .trash:
                var destination: NSURL?
                try FileManager.default.trashItem(at: url, resultingItemURL: &destination)
                return destination.map { [$0 as URL] } ?? []
            default: break
            }
            return []
        }
    }

    /// An empty result indicates that the sheet was cancelled.
    public func pickPaths(
        _ selection: FileSelection = .files,
        multiple: Bool = false,
        types: [UTType] = [],
        directory: URL? = nil,
        title: String? = nil,
        prompt: String? = nil
    ) async throws -> [URL] {
        let panel = NSOpenPanel()
        panel.canChooseFiles = selection != .folders
        panel.canChooseDirectories = selection != .files
        panel.allowsMultipleSelection = multiple
        panel.allowedContentTypes = panel.canChooseFiles ? types : []
        panel.directoryURL = directory
        if let title { panel.title = title }
        if let prompt { panel.prompt = prompt }
        let accepted = try await presentFilePanel(panel)
        return accepted ? panel.urls : []
    }

    /// A nil result indicates that the sheet was cancelled.
    public func pickSavePath(
        fileName: String? = nil,
        types: [UTType] = [],
        directory: URL? = nil,
        title: String? = nil,
        prompt: String? = nil
    ) async throws -> URL? {
        let panel = NSSavePanel()
        panel.allowedContentTypes = types
        panel.directoryURL = directory
        panel.canCreateDirectories = true
        if let fileName { panel.nameFieldStringValue = fileName }
        if let title { panel.title = title }
        if let prompt { panel.prompt = prompt }
        let accepted = try await presentFilePanel(panel)
        return accepted ? panel.url : nil
    }

    private func presentFilePanel(_ panel: NSSavePanel) async throws -> Bool {
        let mountedRoot = root
        guard mountedRoot != 0 else {
            throw AppKitError.invalid("file panel requires a mounted root")
        }
        let owner = window
        let sheetOwner = panelSheetOwner(preferredSheetWindow?() ?? owner)
        guard systemAlerts.isEmpty, activeFilePanel == nil, sheetOwner?.attachedSheet == nil else {
            throw AppKitError.invalid("the window already has an active sheet")
        }
        activeFilePanel = panel
        let result: NSApplication.ModalResponse
        if let sheetOwner {
            result = await panel.beginSheetModal(for: sheetOwner)
        } else {
            NSApp.activate()
            result = await panel.begin()
        }
        panel.orderOut(nil)
        guard activeFilePanel === panel else { return false }
        activeFilePanel = nil
        return result == .OK && root == mountedRoot && window === owner
    }

    private func cancelFilePanel() {
        guard let panel = activeFilePanel else { return }
        activeFilePanel = nil
        if let parent = panel.sheetParent {
            parent.endSheet(panel, returnCode: .cancel)
        } else {
            panel.cancel(nil)
        }
    }

    public override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        guard newSize != lastSize else { return }
        lastSize = newSize
        invalidateEnvironment()
    }

    public override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        invalidateEnvironment(refreshAppearance: true)
    }

    public override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        invalidateEnvironment(refreshAppearance: true)
    }

    public override func layout() {
        super.layout()
        positionPerformancePanel()
        guard !publishing else { return }
        updatePopoverPositions()
        if let window, hasHiddenTitleBar(window) { scheduleWindowState() }
    }

    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        let nextWindow = window
        if observedWindow !== nextWindow {
            cancelSystemAlerts(attachedOnly: true)
            cancelFilePanel()
            observedWindow = nextWindow
            observeWindowState()
            restoreKeyViewLinks()
            keyViewLoopDialogID = nil
            previousFocus = nil
            dialogFocusWindow = nextWindow
            if let nextWindow,
               activeDialogID != nil,
               let responder = nextWindow.firstResponder as? NSView,
               !responder.isDescendant(of: self) {
                previousFocus = responder
            }
            updatePopoverPositions()
            updateDialogKeyViewLoop()
            if let nextWindow,
               let activeDialogID,
               let dialog = views[activeDialogID] as? JadixOverlayView,
               dialog.isOpen {
                if !focusFirst(in: dialog.contentHost) { _ = nextWindow.makeFirstResponder(dialog) }
            }
        }
        applyRequestedFocus()
    }

    private func observeWindowState() {
        let center = NotificationCenter.default
        for observer in windowStateObservers { center.removeObserver(observer) }
        windowStateObservers.removeAll()
        normalWindowFrame = nil
        zoomingWindow = false
        guard root != 0 else { return }
        if let window {
            if !window.isZoomed, !window.isMiniaturized, !window.styleMask.contains(.fullScreen) {
                normalWindowFrame = window.frame
            }
            let changes = [
                NSWindow.didMoveNotification, NSWindow.didResizeNotification,
                NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification,
                NSWindow.didChangeScreenNotification,
                NSWindow.didChangeOcclusionStateNotification, NSWindow.didChangeBackingPropertiesNotification,
                NSWindow.didMiniaturizeNotification, NSWindow.didDeminiaturizeNotification,
                NSWindow.didEnterFullScreenNotification, NSWindow.didExitFullScreenNotification,
            ]
            windowStateObservers = changes.map { name in
                center.addObserver(forName: name, object: window, queue: .main) { [weak self] _ in
                    self?.scheduleWindowState()
                    if name == NSWindow.didBecomeKeyNotification || name == NSWindow.didResignKeyNotification {
                        self?.refreshHandoff()
                        self?.refreshWindowMenu()
                    }
                }
            }
            windowStateObservers.append(center.addObserver(
                forName: NSApplication.didChangeScreenParametersNotification,
                object: NSApplication.shared, queue: .main
            ) { [weak self] _ in self?.scheduleWindowState() })
        }
        refreshHandoff()
        refreshWindowMenu()
        scheduleWindowState()
    }

    /// Call from the containing window delegate's `windowShouldZoom(_:toFrame:)`.
    public func prepareForWindowZoom() {
        guard let window, !window.isMiniaturized,
              !window.styleMask.contains(.fullScreen) else { return }
        if window.isZoomed {
            zoomingWindow = false
            return
        }
        normalWindowFrame = window.frame
        zoomingWindow = true
    }

    /// Forward window delegate events on the main actor before disposing the root.
    public func reportWindowEvent(_ event: JadixWindowEvent) {
        guard root != 0 else { return }
        do {
            let data = try JSONSerialization.data(withJSONObject: event.payload)
            let json = String(decoding: data, as: UTF8.self)
            guard json.withCString({ jadix_native_window_event(root, $0) }) else {
                throw AppKitError.invalid("native window event delivery failed")
            }
        } catch {
            onError?(error)
        }
    }

    func scheduleWindowState() {
        let mountedRoot = root
        guard mountedRoot != 0, pendingWindowStateRoot != mountedRoot else { return }
        pendingWindowStateRoot = mountedRoot
        DispatchQueue.main.async { [weak self] in
            guard let self, self.pendingWindowStateRoot == mountedRoot else { return }
            self.pendingWindowStateRoot = nil
            guard self.root == mountedRoot else { return }
            let json: String
            var observation: [String: Any]?
            if let window = self.window {
                let mode: JadixWindowRequest.Mode
                if window.isMiniaturized { mode = .minimized }
                else if window.styleMask.contains(.fullScreen) { mode = .fullScreen }
                else if window.isZoomed { mode = .maximized }
                else { mode = .normal }
                let frame = window.frame
                if mode != .normal {
                    self.zoomingWindow = false
                } else if !self.zoomingWindow {
                    self.normalWindowFrame = frame
                }
                let contentSize = window.contentRect(forFrameRect: frame).size
                func encodeFrame(_ rect: NSRect) -> [String: Double] {
                    ["x": Double(rect.minX), "y": Double(rect.minY),
                     "width": Double(rect.width), "height": Double(rect.height)]
                }
                var state: [String: Any] = [
                    "frame": encodeFrame(frame),
                    "content_width": Double(contentSize.width),
                    "content_height": Double(contentSize.height),
                    "backing_scale": Double(window.backingScaleFactor),
                    "mode": mode.rawValue,
                    "key": window.isKeyWindow,
                    "visible": window.isVisible,
                    "occluded": !window.occlusionState.contains(.visible),
                ]
                if let normalFrame = self.normalWindowFrame { state["normal_frame"] = encodeFrame(normalFrame) }
                state["title_bar_insets"] = self.titleBarInsets(in: window)
                if let id = window.identifier?.rawValue { state["id"] = id }
                if let screen = window.screen {
                    var display: [String: Any] = [
                        "frame": encodeFrame(screen.frame), "work_area": encodeFrame(screen.visibleFrame),
                    ]
                    let displayKey = NSDeviceDescriptionKey(rawValue: "NSScreenNumber")
                    if let id = screen.deviceDescription[displayKey] as? NSNumber {
                        display["id"] = id.uint32Value
                    }
                    state["display"] = display
                }
                guard let data = try? JSONSerialization.data(withJSONObject: state) else { return }
                json = String(decoding: data, as: UTF8.self)
                observation = state
            } else {
                json = "null"
            }
            if !json.withCString({ jadix_native_window_state(mountedRoot, $0) }) {
                self.onError?(AppKitError.invalid("native window state update failed"))
            } else if self.root == mountedRoot {
                self.onWindowStateChange?(observation)
            }
        }
    }

    private func titleBarInsets(in window: NSWindow) -> [String: Double] {
        var insets = ["top": 0.0, "left": 0.0, "right": 0.0]
        guard hasHiddenTitleBar(window), !window.styleMask.contains(.fullScreen) else { return insets }
        let frame = window.frame
        let style = window.styleMask.subtracting(.fullSizeContentView)
        let naturalContent = NSWindow.contentRect(forFrameRect: frame, styleMask: style)
        let naturalHeight = frame.height - naturalContent.height
        let toolbarHeight = frame.height - window.contentLayoutRect.maxY
        insets["top"] = Double(max(0, naturalHeight, toolbarHeight))
        let buttons: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]
        for kind in buttons {
            guard let button = window.standardWindowButton(kind),
                  button.window === window, !button.isHiddenOrHasHiddenAncestor else { continue }
            let buttonRect = button.convert(button.bounds, to: nil)
            let padding: CGFloat = 8
            if buttonRect.midX <= frame.width / 2 {
                let distance = min(frame.width, buttonRect.maxX + padding)
                insets["left"] = max(insets["left", default: 0], Double(distance))
            } else {
                let distance = min(frame.width, frame.width - buttonRect.minX + padding)
                insets["right"] = max(insets["right", default: 0], Double(distance))
            }
        }
        return insets
    }

    private func invalidateEnvironment(refreshAppearance: Bool = false) {
        guard root != 0 else { return }
        if refreshAppearance { appearanceRevision &+= 1 }
        environment &+= 1
        do {
            let data = try JSONSerialization.data(withJSONObject: jadixEnvironmentSnapshot(self))
            let json = String(decoding: data, as: UTF8.self)
            _ = try json.withCString { try reply(jadix_native_environment(root, $0)) }
            _ = try reply(jadix_native_resize(root, Float(bounds.width), Float(bounds.height), environment))
        } catch { onError?(error) }
    }

    private func scheduleImageLayout() {
        let imageRoot = root
        guard imageRoot != 0, pendingImageLayoutRoot != imageRoot else { return }
        pendingImageLayoutRoot = imageRoot
        DispatchQueue.main.async { [weak self] in
            guard let self, self.pendingImageLayoutRoot == imageRoot else { return }
            self.pendingImageLayoutRoot = nil
            if self.root == imageRoot { self.invalidateEnvironment() }
        }
    }

    deinit {
        MainActor.assumeIsolated {
            let previous = root
            root = 0
            callbackContext.host = nil
            if previous != 0 { jadix_native_dispose(previous) }
            clear()
        }
    }

    private func reply(_ pointer: UnsafeMutablePointer<CChar>?) throws -> String {
        guard let pointer else { throw AppKitError.invalid("missing native reply") }
        defer { jadix_native_string_free(pointer) }
        let value = try object(String(cString: pointer))
        guard value["ok"] as? Bool == true else {
            throw AppKitError.invalid(value["error"] as? String ?? "native failure")
        }
        return value["value"] as? String ?? ""
    }

    private func object(_ text: String) throws -> [String: Any] {
        guard let data = text.data(using: .utf8),
              let value = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw AppKitError.invalid("invalid native JSON")
        }
        return value
    }

    private func make(_ node: [String: Any]) throws -> NSView {
        switch node["kind"] as? String {
        case "hosted": return JadixHostedView()
        case "box": return JadixContainerView()
        case "dialog": return JadixDialogView()
        case "popover":
            let view = JadixPopoverView()
            view.hidePanel()
            return view
        case "scroll":
            let scroll = JadixScrollView()
            scroll.hasVerticalScroller = true
            scroll.hasHorizontalScroller = true
            scroll.drawsBackground = false
            let document = JadixContainerView()
            document.frame = NSRect(origin: .zero, size: scroll.contentSize)
            scroll.documentView = document
            return scroll
        case "text":
            let label = JadixLabel(frame: .zero)
            label.isEditable = false
            label.isSelectable = false
            label.isBezeled = false
            label.isBordered = false
            label.drawsBackground = false
            return label
        case "button":
            let button = JadixRadioButton(title: "", target: self, action: #selector(activate(_:)))
            button.pointerPresence = { [weak self, weak button] inside in
                guard let self, let button else { return }
                self.emit(button, ["kind": "hover", "inside": inside])
            }
            button.pointerMotion = { [weak self, weak button] point in
                guard let self, let button else { return }
                let position = button.window?.contentView?.convert(point, from: nil) ?? point
                self.emit(button, ["kind": "pointerMove", "x": position.x, "y": position.y])
            }
            button.bezelStyle = .rounded
            button.moveInGroup = { [weak self, weak button] keyCode in
                guard let self, let button else { return false }
                return self.moveRadio(button, keyCode: keyCode)
            }
            return button
        case "input":
            let filter: JadixFieldEditFilter = { [weak self] input, editor, range, replacement in
                self?.allowsEdit(of: input, in: editor, range: range, replacement: replacement) ?? true
            }
            let field: NSTextField
            if node["password"] as? Bool == true {
                let input = JadixSecureTextField()
                input.editFilter = filter
                input.didAcquireEditor = Self.scheduleFieldEditorAlignment
                field = input
            } else {
                let input = JadixTextField()
                input.editFilter = filter
                input.didAcquireEditor = Self.scheduleFieldEditorAlignment
                field = input
            }
            field.delegate = self
            field.target = self
            field.action = #selector(submit(_:))
            return field
        case "textarea":
            let text = JadixTextView()
            text.delegate = self
            text.allowsUndo = true
            text.isRichText = false
            text.isAutomaticQuoteSubstitutionEnabled = false
            text.isAutomaticDashSubstitutionEnabled = false
            text.isVerticallyResizable = true
            text.textContainer?.widthTracksTextView = true
            return text
        case "checkbox", "switch":
            let button = NSButton(checkboxWithTitle: "", target: self, action: #selector(toggled(_:)))
            if node["kind"] as? String == "switch" { button.setButtonType(.switch) }
            return button
        case "select":
            return NSPopUpButton(frame: .zero, pullsDown: false)
        case "image":
            return JadixImageView(remoteImages: imageLoader())
        default: throw AppKitError.invalid("unsupported AppKit node")
        }
    }

    private func imageLoader() -> AppleRemoteImages {
        if let remoteImages { return remoteImages }
        let loader = AppleRemoteImages(policy: configuredRemoteImagePolicy)
        remoteImages = loader
        return loader
    }

    private func reuseKey(_ node: [String: Any]) -> String? {
        guard let kind = node["kind"] as? String,
              ["text", "box", "scroll", "button", "input", "textarea", "checkbox", "switch", "select", "image"].contains(kind)
        else { return nil }
        return kind == "input" && node["password"] as? Bool == true ? "secure-input" : kind
    }

    private func makeMountedView(_ node: [String: Any]) throws -> NSView {
        if let id = node["id"] as? String, let pending = preboundViews.removeValue(forKey: id) {
            if pending.key == reuseKey(node) { return pending.view }
            recycle(pending.view, key: pending.key)
        }
        if let key = reuseKey(node), let view = recycledViews[key]?.popLast() {
            recycledViewCount -= 1
            return view
        }
        return try make(node)
    }

    private func recycle(_ view: NSView, key: String) {
        guard recycledViewCount < 128, (recycledViews[key]?.count ?? 0) < 32 else { return }
        resetRecycledView(view)
        if let container = view as? JadixContainerView { container.clearAccessibilityGrid() }
        if let label = view as? JadixLabel {
            label.overflowLineLimit = nil; label.truncation = nil; label.textAppearance = nil; label.preferredMaxLayoutWidth = 0
        }
        if let field = view as? NSTextField {
            field.attributedStringValue = NSAttributedString(string: "")
            field.placeholderString = nil
        }
        if let button = view as? NSButton, !(button is NSPopUpButton) {
            button.setButtonType(key == "checkbox" || key == "switch" ? .switch : .momentaryPushIn)
            button.state = .off; button.attributedTitle = NSAttributedString(string: "")
        }
        if let select = view as? NSPopUpButton { select.removeAllItems() }
        if let text = view as? JadixTextView {
            text.unmarkText(); text.string = ""; text.placeholder = ""
            text.setSelectedRange(NSRange(location: 0, length: 0))
        }
        if let image = view as? JadixImageView { image.setSource(nil) }
        if let scroll = view as? JadixScrollView {
            scroll.cancelScrollAnimation()
            scroll.configureZoom(nil); scroll.onZoom = nil; scroll.onWheel = nil
            scroll.contentView.scroll(to: .zero)
            scroll.lastReportedOffset = .zero; scroll.lastReportedViewport = .zero
            scroll.lastScrollTarget = nil
            scroll.lastReportedScale = 1
            scroll.offsetReportPending = false
        }
        recycledViews[key, default: []].append(view); recycledViewCount += 1
    }

    private func resetRecycledView(_ view: NSView) {
        view.frame = .zero
        view.isHidden = false; view.alphaValue = 1; view.menu = nil; view.toolTip = nil
        view.layer?.removeAllAnimations(); view.layer?.transform = CATransform3DIdentity
        view.layer?.opacity = 1; view.layer?.masksToBounds = false
        view.layer?.filters = nil; view.layer?.backgroundFilters = nil; view.layer?.compositingFilter = nil
        view.layer?.isDoubleSided = true
        view.setAccessibilityChildren(nil); view.setAccessibilityRows(nil); view.setAccessibilityColumns(nil)
        view.setAccessibilityVisibleRows(nil); view.setAccessibilityVisibleColumns(nil)
        view.setAccessibilityVisibleCells(nil); view.setAccessibilityHeader(nil)
        view.setAccessibilityHidden(false); view.setAccessibilityOrderedByRow(false)
        view.setAccessibilityLabel(nil)
        view.setAccessibilityHelp(nil)
        view.setAccessibilityRole(nil)
        view.setAccessibilityRoleDescription(nil)
        view.setAccessibilitySelected(false)
        view.setAccessibilityExpanded(false)
        view.setAccessibilityRowCount(0)
        view.setAccessibilityColumnCount(0)
        view.setAccessibilityRowIndexRange(NSRange(location: NSNotFound, length: 0))
        view.setAccessibilityColumnIndexRange(NSRange(location: NSNotFound, length: 0))
        view.setAccessibilityIndex(NSNotFound)
        view.setAccessibilityLinkedUIElements([])
        view.setAccessibilitySelectedChildren(nil)
        view.setAccessibilityColumnHeaderUIElements(nil)
        view.setAccessibilityParent(nil)
    }

    private func color(_ value: Any?, fallback: NSColor) -> NSColor {
        guard let channels = value as? [Double], channels.count == 4 else { return fallback }
        return NSColor(srgbRed: CGFloat(channels[0] / 255), green: CGFloat(channels[1] / 255),
                       blue: CGFloat(channels[2] / 255), alpha: CGFloat(channels[3] / 255))
    }

    private func accessibilityRole(_ value: String) -> (NSAccessibility.Role, String?)? {
        switch value {
        case "radiogroup": return (.radioGroup, nil)
        case "radio": return (.radioButton, nil)
        case "tablist": return (.tabGroup, nil)
        case "tab": return (.radioButton, "tab")
        case "tabpanel": return (.group, "tab panel")
        case "tree": return (.outline, nil)
        case "treeitem": return (.row, "tree item")
        case "listbox": return (.list, nil)
        case "option": return (.row, nil)
        case "heading": return (.heading, nil)
        case "grid": return (.grid, nil)
        case "gridcell": return (.cell, nil)
        case "columnheader": return (.column, "column header")
        default: return nil
        }
    }

    private func clearTextUndoActions(for view: NSView) {
        var targets: [Any] = [view]
        let manager: UndoManager?
        if let field = view as? NSTextField, let editor = field.currentEditor() {
            targets.append(editor)
            manager = editor.undoManager ?? field.undoManager
        } else if let field = view as? NSTextField {
            manager = field.undoManager
        } else if let textView = view as? JadixTextView {
            manager = textView.undoManager
        } else {
            return
        }
        guard let manager else { return }
        for target in targets { manager.removeAllActions(withTarget: target) }
    }

    private func applyAppearance(_ node: [String: Any], to view: NSView) throws {
        guard let style = node["style"] as? [String: Any],
              let fontSize = style["font_size"] as? Double,
              let fontWeight = style["font_weight"] as? Double else {
            throw AppKitError.invalid("invalid AppKit style")
        }
        let textAppearance = JadixTextAppearance(style["text"])
        if let id = nodeID(for: view) {
            let editor = ((view as? NSTextField)?.currentEditor() as? NSTextView) ?? (view as? NSTextView)
            if editor?.hasMarkedText() == true {
                deferredEditorAppearances.insert(id)
            } else {
                deferredEditorAppearances.remove(id)
            }
        }
        let font = textAppearance.font(resolvedFont(
            families: style["font_family"] as? String ?? "system-ui",
            size: CGFloat(fontSize), weight: fontWeight
        ))
        view.wantsLayer = true
        let visual = JadixVisualStyle(style["view"] as? [String: Any], background: color(style["background"], fallback: .clear))
        let background = visual.background
        if let control = view as? NSControl {
            control.controlSize = .regular
            control.focusRingType = .default
            if let button = control as? NSButton {
                button.contentTintColor = visual.accent ?? visual.tint
                if node["kind"] as? String == "button" || button is NSPopUpButton {
                    button.isBordered = visual.control == "native"
                    button.bezelColor = visual.control == "native" ? nil : background
                }
            }
            if let field = control as? NSTextField, node["kind"] as? String == "input" {
                field.isBordered = visual.control == "native"; field.isBezeled = visual.control == "native"
                field.drawsBackground = visual.control == "native"
            }
        }
        view.layer?.backgroundColor = background.cgColor
        // The shared style uses opaque black as its default foreground.
        let authoredForeground = style["foreground"] as? [Int]
        let usesSystemForeground = authoredForeground == nil || authoredForeground == [0, 0, 0, 255]
        let defaultForeground: NSColor = node["enabled"] as? Bool == false ? .disabledControlTextColor
            : view is JadixTextView || node["kind"] as? String == "input" ? .textColor : .labelColor
        let foreground = usesSystemForeground && background.alphaComponent == 0
            ? defaultForeground : color(style["foreground"], fallback: defaultForeground)
        if let field = view as? NSTextField {
            let composing = (field.currentEditor() as? NSTextView)?.hasMarkedText() == true
            if !composing {
                field.textColor = foreground
                if field.font != font { field.font = font }
            }
            if node["kind"] as? String == "text" {
                applyLabelText(node["text"] as? String ?? "", style: style, to: field, font: font, foreground: foreground)
            } else {
                field.backgroundColor = background.alphaComponent == 0 ? .textBackgroundColor : background
                if !composing {
                    field.alignment = textAppearance.paragraph().alignment
                    field.baseWritingDirection = textAppearance.direction
                }
                if let editor = field.currentEditor() as? NSTextView {
                    textAppearance.applyEditor(editor, font: font, foreground: foreground)
                }
            }
        } else if let text = view as? NSTextView {
            text.drawsBackground = visual.control == "native"
            text.backgroundColor = background.alphaComponent == 0 ? .textBackgroundColor : background
            if !text.hasMarkedText() {
                text.textColor = foreground
                if text.font != font { text.font = font }
            }
            textAppearance.applyEditor(text, font: font, foreground: foreground)
            (text as? JadixTextView)?.placeholderColor = JadixVisualStyle.color(textAppearance.values["placeholder_color"]) ?? .placeholderTextColor
        } else if let control = view as? NSControl, control.font != font {
            control.font = font
        }
        if let button = view as? NSButton, !(button is NSPopUpButton) {
            let title = node["text"] as? String ?? ""
            if style["text"] is [String: Any] || (!usesSystemForeground && (node["enabled"] as? Bool != false || visual.control == "consistent")) {
                button.attributedTitle = textAppearance.display(NSAttributedString(
                    string: title, attributes: textAppearance.attributes(font: font, foreground: foreground)
                ))
            } else {
                button.attributedTitle = NSAttributedString(string: title)
            }
        }
        if let select = view as? NSPopUpButton {
            for item in select.itemArray {
                let attributes = textAppearance.attributes(font: font, foreground: item.isEnabled && select.isEnabled
                    ? foreground : NSColor.disabledControlTextColor)
                item.attributedTitle = textAppearance.display(NSAttributedString(string: item.title, attributes: attributes))
            }
        }
    }

    private func resolvedFont(families: String, size: CGFloat, weight: Double) -> NSFont {
        JadixTextAppearance.resolvedFont(families: families, size: size, weight: weight)
    }

    private func applyLabelText(
        _ source: String, style: [String: Any], to label: NSTextField, font: NSFont, foreground: NSColor
    ) {
        let overflow: (NSLineBreakMode, CTLineTruncationType?)
        switch style["text_overflow"] as? String ?? "wrap" {
        case "clip": overflow = (.byClipping, nil)
        case "ellipsis": overflow = (.byTruncatingTail, .end)
        case "ellipsis_start": overflow = (.byTruncatingHead, .start)
        case "ellipsis_middle": overflow = (.byTruncatingMiddle, .middle)
        default: overflow = (.byWordWrapping, nil)
        }
        let (lineBreak, truncation) = overflow
        let nativeLabel = label as? JadixLabel
        let textAppearance = JadixTextAppearance(style["text"])
        nativeLabel?.textAppearance = style["text"] is [String: Any] ? textAppearance : nil
        let maxLines = style["max_lines"] as? Int ?? (lineBreak == .byWordWrapping ? 0 : 1)
        let ranges = style["inline_styles"] as? [[String: Any]] ?? []
        let direction: NSWritingDirection
        switch style["direction"] as? String {
        case "right-to-left": direction = .rightToLeft
        case "left-to-right": direction = .leftToRight
        default: direction = textAppearance.direction
        }
        nativeLabel?.overflowLineLimit = lineBreak == .byWordWrapping ? nil : max(1, maxLines)
        nativeLabel?.truncation = truncation
        guard !ranges.isEmpty || direction != .natural || lineBreak != .byWordWrapping || style["text"] is [String: Any] else {
            label.stringValue = source
            label.lineBreakMode = lineBreak
            label.maximumNumberOfLines = maxLines
            return
        }
        let paragraph = textAppearance.paragraph(lineBreak: lineBreak)
        paragraph.baseWritingDirection = direction
        if textAppearance.values["align"] == nil, direction != .natural { paragraph.alignment = direction == .rightToLeft ? .right : .left }
        var baseAttributes = textAppearance.attributes(font: font, foreground: foreground, lineBreak: lineBreak)
        baseAttributes[.paragraphStyle] = paragraph
        let styled = NSMutableAttributedString(
            string: source, attributes: baseAttributes
        )
        if lineBreak != .byWordWrapping {
            let text = source as NSString
            let left = NSMutableParagraphStyle()
            left.setParagraphStyle(paragraph)
            left.alignment = paragraph.alignment == .natural ? .left : paragraph.alignment
            let right = NSMutableParagraphStyle()
            right.setParagraphStyle(paragraph)
            let alignment = textAppearance.values["align"] as? String ?? "start"
            right.alignment = alignment == "start" ? .right : alignment == "end" ? .left : paragraph.alignment
            left.baseWritingDirection = .leftToRight
            right.baseWritingDirection = .rightToLeft
            var start = 0
            while start < text.length {
                var end = 0
                var contentsEnd = 0
                text.getParagraphStart(nil, end: &end, contentsEnd: &contentsEnd,
                                       for: NSRange(location: start, length: 0))
                let contents = NSRange(location: start, length: contentsEnd - start)
                let rtl = direction == .rightToLeft
                    || (direction == .natural && JadixLabel.startsRightToLeft(source, range: contents))
                styled.addAttribute(.paragraphStyle, value: rtl ? right : left,
                                    range: NSRange(location: start, length: end - start))
                start = end
            }
        }
        var index = source.startIndex
        var utf8Offset = 0
        var utf16Offset = 0
        func advance(to target: Int) {
            while utf8Offset < target, index < source.endIndex {
                let next = source.index(after: index)
                let character = source[index..<next]
                utf8Offset += character.utf8.count
                utf16Offset += character.utf16.count
                index = next
            }
        }
        let fonts = NSFontManager.shared
        for span in ranges {
            guard let range = span["range"] as? [Int], range.count == 2 else { continue }
            advance(to: range[0])
            let start = utf16Offset
            advance(to: range[1])
            guard start < utf16Offset else { continue }
            var attributes: [NSAttributedString.Key: Any] = [:]
            if let value = span["foreground"] { attributes[.foregroundColor] = color(value, fallback: foreground) }
            if let value = span["background"] { attributes[.backgroundColor] = color(value, fallback: .clear) }
            var spanFont = font
            if let bold = span["bold"] as? Bool {
                spanFont = bold ? fonts.convert(spanFont, toHaveTrait: .boldFontMask)
                    : fonts.convert(spanFont, toNotHaveTrait: .boldFontMask)
            }
            if let italic = span["italic"] as? Bool {
                spanFont = italic ? fonts.convert(spanFont, toHaveTrait: .italicFontMask)
                    : fonts.convert(spanFont, toNotHaveTrait: .italicFontMask)
            }
            if spanFont != font, let cascade = font.fontDescriptor.object(forKey: .cascadeList) {
                let descriptor = spanFont.fontDescriptor.addingAttributes([.cascadeList: cascade])
                spanFont = NSFont(descriptor: descriptor, size: spanFont.pointSize) ?? spanFont
            }
            attributes[.font] = spanFont
            if let value = span["underline"] as? Bool { attributes[.underlineStyle] = value ? NSUnderlineStyle.single.rawValue : 0 }
            if let value = span["strikethrough"] as? Bool { attributes[.strikethroughStyle] = value ? NSUnderlineStyle.single.rawValue : 0 }
            styled.addAttributes(attributes, range: NSRange(location: start, length: utf16Offset - start))
        }
        label.lineBreakMode = lineBreak
        label.maximumNumberOfLines = maxLines
        label.attributedStringValue = textAppearance.display(styled)
    }

    private func apply(_ node: [String: Any], to view: NSView, preserveComposition: Bool = false) throws {
        try applyAppearance(node, to: view)
        let previous = (node["id"] as? String).flatMap { descriptors[$0] }
        view.setAccessibilityLabel(node["name"] as? String)
        view.setAccessibilityHelp(node["description"] as? String)
        if previous?["role"] as? String == "gridcell", node["role"] as? String != "gridcell" {
            view.setAccessibilityColumnHeaderUIElements(nil)
        }
        if let roleName = node["role"] as? String,
           let (role, roleDescription) = accessibilityRole(roleName) {
            view.setAccessibilityRole(role)
            view.setAccessibilityRoleDescription(roleDescription)
        }
        view.setAccessibilitySelected(node["selected"] as? Bool ?? false)
        view.setAccessibilityExpanded(node["expanded"] as? Bool ?? false)
        if let grid = node["grid"] as? [String: Int] {
            view.setAccessibilityRowCount(grid["rows"] ?? 0)
            view.setAccessibilityColumnCount(grid["columns"] ?? 0)
            view.setAccessibilityOrderedByRow(true)
        } else if previous?["grid"] is [String: Int] {
            view.setAccessibilityRowCount(0)
            view.setAccessibilityColumnCount(0)
            view.setAccessibilityOrderedByRow(false)
        }
        if let cell = node["cell"] as? [String: Int] {
            view.setAccessibilityRowIndexRange(NSRange(location: cell["row"] ?? 0, length: cell["rowSpan"] ?? 1))
            view.setAccessibilityColumnIndexRange(NSRange(location: cell["column"] ?? 0, length: cell["columnSpan"] ?? 1))
        } else if let header = node["columnHeader"] as? [String: Int] {
            view.setAccessibilityRowIndexRange(NSRange(location: 0, length: 1))
            view.setAccessibilityColumnIndexRange(NSRange(location: header["column"] ?? 0, length: header["columnSpan"] ?? 1))
        } else if previous?["cell"] is [String: Int] || previous?["columnHeader"] is [String: Int] {
            view.setAccessibilityRowIndexRange(NSRange(location: NSNotFound, length: 0))
            view.setAccessibilityColumnIndexRange(NSRange(location: NSNotFound, length: 0))
        }
        if let column = node["columnHeader"] as? [String: Int] {
            view.setAccessibilityIndex(column["column"] ?? 0)
        } else if previous?["columnHeader"] is [String: Int] {
            view.setAccessibilityIndex(NSNotFound)
        }
        if let button = view as? NSButton, !(button is NSPopUpButton) {
            button.isEnabled = node["enabled"] as? Bool ?? true
            if let button = button as? JadixRadioButton {
                button.takesKeyboardFocus = !(node["role"] as? String == "option" && node["tabStop"] as? Bool == false)
                button.activatesOnPress = !button.takesKeyboardFocus
            }
            let isRadio = node["role"] as? String == "radio"
            if isRadio {
                button.setButtonType(.radio)
            } else if previous?["role"] as? String == "radio" {
                button.setButtonType(.momentaryPushIn)
                button.bezelStyle = .rounded
            }
            let selected = node["selected"] as? Bool ?? node["checked"] as? Bool ?? false
            button.state = selected ? .on : .off
        }
        if node["kind"] as? String == "input", let input = view as? NSTextField {
            guard (input is NSSecureTextField) == (node["password"] as? Bool ?? false) else {
                throw AppKitError.invalid("input security mode changed")
            }
            let value = node["text"] as? String ?? ""
            let editor = input.currentEditor() as? NSTextInputClient
            if input.stringValue != value && !(preserveComposition && editor?.hasMarkedText() == true) {
                input.stringValue = value
                clearTextUndoActions(for: input)
            }
            input.placeholderString = node["placeholder"] as? String
            if let placeholder = input.placeholderString,
               let style = node["style"] as? [String: Any] {
                let appearance = JadixTextAppearance(style["text"])
                input.placeholderAttributedString = NSAttributedString(string: placeholder,
                    attributes: appearance.attributes(font: input.font ?? .systemFont(ofSize: 16),
                        foreground: JadixVisualStyle.color(appearance.values["placeholder_color"]) ?? .placeholderTextColor))
            }
            input.isEditable = true
            input.isSelectable = true
            input.isEnabled = node["enabled"] as? Bool ?? false
            if input.currentEditor() == nil, let style = node["style"] as? [String: Any] {
                input.attributedStringValue = NSAttributedString(string: input.stringValue,
                    attributes: JadixTextAppearance(style["text"]).attributes(
                        font: input.font ?? .systemFont(ofSize: 16), foreground: input.textColor ?? .textColor))
            }
        }
        if let text = view as? JadixTextView {
            let value = node["text"] as? String ?? ""
            if text.string != value && !(preserveComposition && text.hasMarkedText()) {
                text.string = value
                clearTextUndoActions(for: text)
            }
            text.placeholder = node["placeholder"] as? String ?? ""
            text.isEditable = node["enabled"] as? Bool ?? false
            text.isSelectable = text.isEditable
            if let style = node["style"] as? [String: Any] {
                JadixTextAppearance(style["text"]).applyEditor(text,
                    font: text.font ?? .systemFont(ofSize: 16), foreground: text.textColor ?? .textColor)
            }
        }
        if let select = view as? NSPopUpButton {
            select.isEnabled = node["enabled"] as? Bool ?? false
            let selected = node["value"] as? String ?? ""
            let options = (node["options"] as? [[String: Any]] ?? []).compactMap { option
                -> (title: String, value: String, enabled: Bool)? in
                guard let title = option["label"] as? String, let value = option["value"] as? String else { return nil }
                return (title, value, option["enabled"] as? Bool ?? true)
            }
            let selectedIndex = options.firstIndex { $0.value == selected }
            let placeholder = node["placeholder"] as? String ?? ""
            let showsPlaceholder = selectedIndex == nil && !placeholder.isEmpty
            select.menu?.autoenablesItems = false
            select.removeAllItems()
            // AppKit replaces existing items with the same title during insertion.
            for index in 0..<(options.count + (showsPlaceholder ? 1 : 0)) {
                select.addItem(withTitle: String(index))
            }
            for (item, option) in zip(select.itemArray, options) {
                item.title = option.title
                item.representedObject = option.value
                item.isEnabled = option.enabled
            }
            if let selectedIndex {
                select.selectItem(at: selectedIndex)
            } else if showsPlaceholder, let item = select.item(at: options.count) {
                item.title = placeholder
                item.isEnabled = false
                select.select(item)
            } else {
                select.select(nil)
            }
            select.target = self
            select.action = #selector(selected(_:))
            try applyAppearance(node, to: select)
        }
        if let image = view as? JadixImageView {
            image.setAccessibilityLabel(node["name"] as? String ?? node["alt"] as? String)
        }
    }

    func measure(_ json: String) throws -> (NSSize, CGFloat?) {
        let request = try object(json)
        guard let node = request["node"] as? [String: Any],
              let width = request["maxWidth"] as? Double,
              let height = request["maxHeight"] as? Double else {
            throw AppKitError.invalid("invalid AppKit measure")
        }
        if node["kind"] as? String == "hosted" {
            let specification = node["specification"] as? [String: Any] ?? [:]
            let type = specification["type"] as? String
            let controls = type == "menu_button" || type == "segments"
            let labels = (specification["items"] as? [[String: Any]] ?? []).compactMap { $0["label"] as? String }
            let text = specification["label"] as? String ?? labels.joined(separator: "    ")
            let naturalWidth = controls ? (text as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: NSFont.systemFontSize)]).width + 36 : 320
            return (NSSize(width: min(width, naturalWidth), height: min(height, controls ? 28 : 180)), nil)
        }
        if node["kind"] as? String == "image" {
            guard let id = request["id"] as? String,
                  let source = node["source"] as? String,
                  let size = (views[id] as? JadixImageView)?.originalSize(for: source) else { return (.zero, nil) }
            let style = node["style"] as? [String: Any]
            let specifiedWidth = style?["width"] as? Double
            let specifiedHeight = style?["height"] as? Double
            let originalWidth = Double(size.width)
            let originalHeight = Double(size.height)
            let scale = specifiedWidth.map { min(width, $0) / originalWidth }
                ?? specifiedHeight.map { min(height, $0) / originalHeight }
                ?? 1
            let constrainedScale = min(scale, width / originalWidth, height / originalHeight)
            return (NSSize(width: originalWidth * constrainedScale, height: originalHeight * constrainedScale), nil)
        }
        let measurementID = request["id"] as? String
        let key = reuseKey(node)
        let view: NSView
        if let id = measurementID, let pending = preboundViews[id], pending.key == key {
            view = pending.view
        } else {
            view = try makeMountedView(node)
            if let id = measurementID, let key, views[id] == nil, preboundViews.count < 256 {
                preboundViews[id] = (key, view)
            }
        }
        try apply(node, to: view)
        let fitting: NSSize
        let overflow = (view as? JadixLabel)?.overflowLayout(width: CGFloat(width), height: CGFloat(height))
        if let overflow {
            fitting = overflow.size
        } else if node["kind"] as? String == "text", let label = view as? NSTextField {
            let naturalWidth = label.intrinsicContentSize.width
            label.preferredMaxLayoutWidth = naturalWidth > 0 ? min(CGFloat(width), naturalWidth) : CGFloat(width)
            fitting = label.intrinsicContentSize
        } else {
            fitting = view.fittingSize
        }
        let measuredWidth = request["widthMode"] as? String == "exact" ? width : min(width, Double(fitting.width))
        let measuredHeight = request["heightMode"] as? String == "exact" ? height : min(height, Double(fitting.height))
        let size = NSSize(width: measuredWidth, height: measuredHeight)
        if let overflow {
            let offset = (view as? JadixLabel)?.textAppearance?.verticalOffset(available: size.height, used: overflow.size.height) ?? 0
            let baseline = overflow.lines.first.map { $0.baseline + offset }
            return (size, baseline.flatMap { $0 > 0 && $0 <= size.height ? $0 : nil })
        }
        guard let label = view as? JadixLabel, !label.stringValue.isEmpty, size.height > 0 else {
            return (size, nil)
        }
        label.setFrameSize(size)
        let alignmentRect = label.alignmentRect(forFrame: label.frame)
        let baseline = label.frame.maxY - alignmentRect.maxY + label.firstBaselineOffsetFromTop
        return (size, baseline.isFinite && baseline > 0 && baseline <= size.height ? baseline : nil)
    }

    fileprivate func publish(_ json: String) throws {
        let snapshot = try object(json)
        guard snapshot["protocol"] as? Int == 1,
              let token = snapshot["root"] as? String,
              root == 0 || token == String(root) else {
            throw AppKitError.invalid("invalid AppKit snapshot")
        }
        if let address = snapshot["appAddress"] {
            guard address is NSNull || address is String else {
                throw AppKitError.invalid("invalid AppKit app address")
            }
            applyAppAddress(address as? String)
            return
        }
        if let menuBar = snapshot["menuBar"] {
            guard root != 0 else { throw AppKitError.invalid("unmounted native root") }
            let data = try JSONSerialization.data(withJSONObject: menuBar)
            let menu = try JSONDecoder().decode(JadixMenuBarRequest.self, from: data)
            applyWindowMenu(menu)
            return
        }
        if let command = snapshot["imageCache"] as? [String: Any] {
            guard root != 0, let bytes = command["bytes"] as? Int, (0...33554432).contains(bytes),
                  let entries = command["entries"] as? Int, (0...128).contains(entries),
                  let age = command["age"] as? Double, age.isFinite, (0...86400).contains(age) else { throw AppKitError.invalid("invalid image cache budget") }
            let loader = imageLoader()
            Task { @MainActor in await loader.configureCache(bytes: bytes, entries: entries, age: age) }
            return
        }
        if let command = snapshot["desktopService"] as? [String: Any] {
            guard root != 0 else { throw AppKitError.invalid("unmounted desktop service") }
            desktopServices.handle(command); return
        }
        if let command = snapshot["fetch"] as? [String: Any] {
            guard root != 0 else { throw AppKitError.invalid("unmounted HTTP service") }
            let mountedRoot = root
            fetchService.event = { [weak self] id, event in
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.root == mountedRoot,
                          let data = try? JSONSerialization.data(withJSONObject: event), data.count <= 400 * 1024,
                          let json = String(data: data, encoding: .utf8) else { return }
                    json.withCString { _ = jadix_native_fetch_event(mountedRoot, id, $0) }
                }
            }
            fetchService.handle(command); return
        }
        if let command = snapshot["speech"] as? [String: Any] {
            guard root != 0 else { throw AppKitError.invalid("unmounted speech service") }
            let mountedRoot = root
            speechService.event = { [weak self] id, event in
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.root == mountedRoot,
                          let data = try? JSONSerialization.data(withJSONObject: event), data.count <= 70 * 1024,
                          let json = String(data: data, encoding: .utf8) else { return }
                    json.withCString { _ = jadix_native_speech_event(mountedRoot, id, $0) }
                }
            }
            speechService.handle(command); return
        }
        if let commands = snapshot["audio"] as? [[String: Any]] {
            guard root != 0 else { throw AppKitError.invalid("unmounted audio service") }
            let mountedRoot = root
            audioService.event = { [weak self] id, event in
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.root == mountedRoot,
                          let data = try? JSONSerialization.data(withJSONObject: event), data.count <= 16 * 1024,
                          let json = String(data: data, encoding: .utf8) else { return }
                    json.withCString { _ = jadix_native_audio_event(mountedRoot, id, $0) }
                }
            }
            audioService.handle(commands); return
        }
        if let tray = snapshot["tray"] as? [String: Any] {
            guard root != 0 else { throw AppKitError.invalid("unmounted status item") }
            let mountedRoot = root
            statusItem.selected = { [weak self] revision, command in
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.root == mountedRoot else { return }
                    command.withCString { _ = jadix_native_tray_command(mountedRoot, revision, $0) }
                }
            }
            statusItem.update(tray); return
        }
        if let command = snapshot["webResource"] as? [String: Any] {
            guard let serial = command["request"] as? String, let request = UInt64(serial),
                  let answer = command["answer"] as? [String: Any] else { throw AppKitError.invalid("invalid web resource response") }
            guard let reply = webResourceReplies.removeValue(forKey: request) else { return }
            if answer["error"] is String { reply(.failure(URLError(.cannotLoadFromNetwork))) }
            else if let status = answer["status"] as? Int, let mime = answer["mime"] as? String,
                    let encoded = answer["bytes"] as? String, encoded.utf8.count <= 44_739_244,
                    let bytes = Data(base64Encoded: encoded), bytes.count <= 32 * 1024 * 1024 {
                reply(.success(JadixWebResourceResponse(status: status, mime: mime, bytes: bytes)))
            } else { reply(.failure(URLError(.badServerResponse))) }
            return
        }
        if let command = snapshot["webEvaluate"] as? [String: Any] {
            guard root != 0, let id = command["node"] as? String,
                  let hosted = views[id] as? JadixHostedView,
                  let serial = command["request"] as? String, let request = UInt64(serial),
                  let script = command["script"] as? String else { throw AppKitError.invalid("invalid web evaluation") }
            let mountedRoot = root
            let generation = hosted.documentGeneration
            DispatchQueue.main.async { [weak self, weak hosted] in
                guard let self, let hosted, self.root == mountedRoot, self.views[id] === hosted, hosted.documentGeneration == generation else { return }
                hosted.evaluate(script) { [weak self] value, error in
                    guard let self, self.root == mountedRoot, self.views[id] === hosted, hosted.documentGeneration == generation else { return }
                    let success = error == nil
                    let text: String
                    if let error { text = String(error.localizedDescription.prefix(4096)) }
                    else if let data = try? JSONSerialization.data(withJSONObject: value ?? NSNull(), options: [.fragmentsAllowed]), data.count <= 1024 * 1024 + 4096 {
                        text = String(data: data, encoding: .utf8) ?? "null"
                    } else { text = "{\"status\":\"error\",\"code\":\"too-large\"}" }
                    text.withCString { _ = jadix_native_web_complete(mountedRoot, request, success, $0) }
                }
            }
            return
        }
        if let command = snapshot["zoomTo"] as? [String: Any] {
            guard root != 0, let id = command["node"] as? String,
                  let scroll = views[id] as? JadixScrollView,
                  let style = descriptors[id]?["style"] as? [String: Any], style["scroll_zoom"] != nil,
                  let scale = command["scale"] as? Double, scale.isFinite, scale > 0,
                  let anchor = command["anchor"] as? [Double], anchor.count == 2,
                  anchor.allSatisfy(\.isFinite) else { throw AppKitError.invalid("invalid AppKit zoom target") }
            scroll.zoom(to: CGFloat(scale), anchor: NSPoint(x: anchor[0], y: anchor[1]))
            return
        }
        if let command = snapshot["stopScroll"] as? [String: Any] {
            guard root != 0, let id = command["node"] as? String,
                  let scroll = views[id] as? JadixScrollView else {
                throw AppKitError.invalid("invalid AppKit scroll target")
            }
            scroll.cancelScrollAnimation(); scroll.contentView.layer?.removeAllAnimations()
            return
        }
        if let command = snapshot["scrollTo"] as? [String: Any] {
            guard root != 0, let id = command["node"] as? String,
                  let scroll = views[id] as? JadixScrollView, scroll.documentView != nil,
                  let offset = command["offset"] as? [Double], offset.count == 2,
                  offset.allSatisfy({ $0.isFinite && $0 >= 0 }) else {
                throw AppKitError.invalid("invalid AppKit scroll target")
            }
            let clip = scroll.contentView
            var destination = clip.bounds
            destination.origin = NSPoint(x: offset[0], y: offset[1])
            scroll.scroll(to: destination.origin, smoothly: false)
            return
        }
        guard let nodes = snapshot["nodes"] as? [[String: Any]] else {
            throw AppKitError.invalid("invalid AppKit snapshot")
        }
        let performanceStart = CACurrentMediaTime()
        publishing = true
        defer {
            publishing = false
            performanceWork += CACurrentMediaTime() - performanceStart
            if performanceVisible { addSubview(performancePanel, positioned: .above, relativeTo: nil) }
        }
        let previousResponder = focusedView()
        let previousPopover = previousResponder.flatMap { containingPopover($0) }
        let snapshotAppearanceRevision = appearanceRevision
        let focusChanged = snapshot["focusChanged"] as? Bool == true
        if focusChanged, let focused = snapshot["focused"] { focusedID = focused as? String }
        nodeOrder = nodes.compactMap { $0["id"] as? String }
        let childrenByParent = Dictionary(grouping: nodes.filter { $0["parent"] is String }) {
            $0["parent"] as? String ?? ""
        }
        let nextNodeIDs = Set(nodeOrder)
        var live = Set<String>()
        for rows in accessibilityGridRows.values { for row in rows.values { row.clear() } }
        for columns in accessibilityGridColumns.values { for column in columns.values { clearAccessibilityColumn(column) } }
        var removedOpenPopovers: [(String, JadixPopoverEvent)] = []
        var retiredViews: [(String, NSView)] = []
        for id in Array(views.keys) where !nextNodeIDs.contains(id) {
            if let observer = scrollObservers.removeValue(forKey: id) { NotificationCenter.default.removeObserver(observer) }
            if let view = views.removeValue(forKey: id) {
                viewAppearances.removeValue(forKey: id)?.remove()
                nodeIDsByView.removeValue(forKey: ObjectIdentifier(view))
                if let popover = view as? JadixPopoverView {
                    popover.hidePanel()
                    if popover.reportedOpen { removedOpenPopovers.append((id, .closed)) }
                }
                (view as? JadixHostedView)?.dispose()
                (view as? JadixImageView)?.setSource(nil)
                if let key = descriptors[id].flatMap(reuseKey) { retiredViews.append((key, view)) }
                (view as? JadixContainerView)?.clearAccessibilityGrid()
                clearTextUndoActions(for: view)
                if focusedView() === view { _ = window?.makeFirstResponder(nil) }
                clearTextUndoActions(for: view)
                view.removeFromSuperview()
            }
            descriptors.removeValue(forKey: id)
            deferredEditorAppearances.remove(id)
            revisions.removeValue(forKey: id)
        }
        for (key, view) in retiredViews {
            if let scroll = view as? JadixScrollView {
                guard scroll.documentView?.subviews.isEmpty != false else { continue }
            } else if !view.subviews.isEmpty { continue }
            recycle(view, key: key)
        }
        var remainingSiblings: [ObjectIdentifier: ArraySlice<NSView>] = [:]
        var lastPlacedSibling: [ObjectIdentifier: NSView] = [:]
        var placedViews = Set<ObjectIdentifier>()
        for node in nodes {
            guard let id = node["id"] as? String,
                  let frame = node["frame"] as? [Double], frame.count == 4 else {
                throw AppKitError.invalid("invalid AppKit node")
            }
            live.insert(id)
            let view: NSView
            var restoredInputSelection: NSRange?
            var restoreInputFirstResponder = false
            if let existing = views[id] {
                if let old = descriptors[id], old["kind"] as? String != node["kind"] as? String {
                    throw AppKitError.invalid("native node kind changed")
                }
                let secure = node["password"] as? Bool ?? false
                if node["kind"] as? String == "input",
                   let field = existing as? NSTextField,
                   (field is NSSecureTextField) != secure {
                    let replacement = try make(node)
                    guard let input = replacement as? NSTextField else {
                        throw AppKitError.invalid("invalid AppKit input replacement")
                    }
                    input.stringValue = field.stringValue
                    input.frame = field.frame
                    let editor = field.currentEditor()
                    restoredInputSelection = editor?.selectedRange
                    restoreInputFirstResponder = field.window.map {
                        $0.firstResponder === field || $0.firstResponder === editor
                    } ?? false
                    clearTextUndoActions(for: field)
                    viewAppearances.removeValue(forKey: id)?.remove()
                    field.superview?.replaceSubview(field, with: input)
                    nodeIDsByView.removeValue(forKey: ObjectIdentifier(field))
                    view = input
                } else {
                    view = existing
                }
            } else {
                view = try makeMountedView(node)
            }
            views[id] = view
            nodeIDsByView[ObjectIdentifier(view)] = id
            if let scroll = view as? JadixScrollView, scroll.onWheel == nil {
                scroll.onWheel = { [weak self, weak scroll] deltaX, deltaY in
                    guard let self, let scroll else { return }
                    self.emit(scroll, ["kind": "wheel", "deltaX": deltaX, "deltaY": deltaY], node: id)
                }
            }
            let parent: NSView
            var parentOrigin = NSPoint.zero
            if let parentID = node["parent"] as? String,
               let parentView = views[parentID],
               let parentFrame = descriptors[parentID]?["frame"] as? [Double] {
                if let overlay = parentView as? JadixOverlayView {
                    parent = overlay.contentHost
                } else if let scroll = parentView as? NSScrollView, let document = scroll.documentView {
                    parent = document
                } else if let hosted = parentView as? JadixHostedView { parent = hosted.content }
                else { parent = parentView }
                parentOrigin = NSPoint(x: parentFrame[0], y: parentFrame[1])
            } else if node["parent"] is NSNull { parent = self }
            else { throw AppKitError.invalid("invalid AppKit parent") }
            if !(view is JadixOverlayView) {
                let parentKey = ObjectIdentifier(parent)
                var siblings = remainingSiblings[parentKey] ?? parent.subviews.filter { candidate in
                    guard !(candidate is JadixOverlayView), let id = nodeID(for: candidate) else { return false }
                    return nextNodeIDs.contains(id)
                }[...]
                while let next = siblings.first, next.superview !== parent || placedViews.contains(ObjectIdentifier(next)) {
                    siblings.removeFirst()
                }
                if siblings.first === view {
                    siblings.removeFirst()
                } else if let next = siblings.first {
                    parent.addSubview(view, positioned: .below, relativeTo: next)
                } else if let last = lastPlacedSibling[parentKey] {
                    parent.addSubview(view, positioned: .above, relativeTo: last)
                } else {
                    parent.addSubview(view, positioned: .below, relativeTo: nil)
                }
                remainingSiblings[parentKey] = siblings
                lastPlacedSibling[parentKey] = view
                placedViews.insert(ObjectIdentifier(view))
            }
            if descriptors[id].map({ NSDictionary(dictionary: $0).isEqual(NSDictionary(dictionary: node)) }) != true {
                let preserveComposition = descriptors[id]?["text"] as? String == node["text"] as? String
                    && node["enabled"] as? Bool == true
                if let previousRole = descriptors[id]?["role"] as? String {
                    let nextRole = node["role"] as? String
                    if nextRole == nil || (nextRole != previousRole && nextRole.flatMap(accessibilityRole) == nil) {
                        view.setAccessibilityRole(nil)
                        view.setAccessibilityRoleDescription(nil)
                    }
                }
                try apply(node, to: view, preserveComposition: preserveComposition)
            } else if appliedAppearanceRevision != snapshotAppearanceRevision {
                try applyAppearance(node, to: view)
            }
            if restoreInputFirstResponder, let input = view as? NSTextField {
                _ = input.window?.makeFirstResponder(input)
                if let selection = restoredInputSelection,
                   let editor = input.currentEditor() {
                    let location = min(selection.location, (input.stringValue as NSString).length)
                    let length = min(selection.length, (input.stringValue as NSString).length - location)
                    editor.selectedRange = NSRange(location: location, length: length)
                }
            }
            let x = frame[0] - parentOrigin.x
            let y = frame[1] - parentOrigin.y
            let nextFrame = NSRect(x: x, y: y, width: frame[2], height: frame[3])
            if let overlay = view as? JadixOverlayView {
                overlay.closeOnOutside = (node["closeOnOverlayClick"] as? Bool)
                    ?? (node["closeOnOutsideClick"] as? Bool ?? false)
                if let popover = overlay as? JadixPopoverView {
                    let style = node["style"] as? [String: Any]
                    popover.appearanceWire = style?["view"] as? [String: Any] ?? [:]
                    popover.visibleAppearance = JadixVisualStyle(popover.appearanceWire, background: popover.appearanceBackground)
                    popover.closeOnEscape = style?["popover_close_on_escape"] as? Bool ?? true
                    popover.enterDuration = (style?["popover_enter_ms"] as? Double ?? 0) / 1000
                    popover.exitDuration = (style?["popover_exit_ms"] as? Double ?? 0) / 1000
                    popover.startingAppearance = .init(style: style?["popover_starting"] as? [String: Any])
                    popover.endingAppearance = .init(style: style?["popover_ending"] as? [String: Any])
                }
                let id = node["id"] as? String
                overlay.onDismiss = { [weak self, weak overlay] backdrop in
                    guard let self, let overlay, let id,
                          let revision = self.revisions[id] else { return }
                    self.emit(overlay, ["kind": "dismiss", "backdrop": backdrop, "revision": String(revision)], node: id)
                }
            } else if view.frame != nextFrame { view.frame = nextFrame }
            if !(view is JadixOverlayView) {
                let style = node["style"] as? [String: Any] ?? [:]
                if let appearance = style["view"] as? [String: Any] {
                    let renderer = viewAppearances[id] ?? JadixViewAppearance(view)
                    viewAppearances[id] = renderer
                    renderer.onFrameWork = { [weak self] seconds in
                        guard let self, !self.publishing else { return }
                        self.performanceWork += seconds
                    }
                    renderer.onTransformChange = { [weak self, weak view] in
                        guard let self, let view, !self.publishing else { return }
                        self.refreshBackingDensity(in: view)
                    }
                    renderer.update(appearance, background: color(style["background"], fallback: .clear))
                } else if let renderer = viewAppearances.removeValue(forKey: id) {
                    renderer.remove()
                    view.layer?.backgroundColor = color(style["background"], fallback: .clear).cgColor
                }
            }
            if let hosted = view as? JadixHostedView {
                let mountedRoot = root
                hosted.embeddedFactory = { [weak self] kind in self?.embeddedFactories[kind] }
                hosted.resourceAllowed = { [weak self, weak hosted] url in
                    guard let self, let hosted, self.root != 0, self.views[id] === hosted else { return false }
                    return url.absoluteString.withCString { jadix_native_web_resource_allowed(self.root, $0) }
                }
                hosted.resourceProvider = { [weak self, weak hosted] request, reply in
                    guard let self, let hosted, self.root != 0, self.views[id] === hosted,
                          self.webResourceReplies.count < 64, let url = request.url else { return nil }
                    let root = self.root
                    let serial = id.withCString { node in url.absoluteString.withCString { url in
                        (request.httpMethod ?? "GET").withCString { jadix_native_web_resource_request(root, node, url, $0) }
                    } }
                    guard serial != 0 else { return nil }
                    self.webResourceReplies[serial] = reply
                    return { [weak self] in
                        self?.webResourceReplies.removeValue(forKey: serial)
                        DispatchQueue.main.async { [weak self] in
                            guard self?.root == root else { return }
                            jadix_native_web_resource_cancel(root, serial)
                        }
                    }
                }
                hosted.emit = { [weak self, weak hosted] event, data in
                    DispatchQueue.main.async { [weak self, weak hosted] in
                        guard let self, let hosted, self.root != 0, (mountedRoot == 0 || self.root == mountedRoot), self.views[id] === hosted else { return }
                        self.emit(hosted, ["kind":"hosted", "event":event, "data":data], node: id)
                    }
                }
                hosted.documentChanged = { [weak self, weak hosted] ready in
                    guard let generation = hosted?.documentGeneration else { return }
                    DispatchQueue.main.async { [weak self, weak hosted] in
                        guard let self, let hosted, self.root != 0, (mountedRoot == 0 || self.root == mountedRoot), self.views[id] === hosted else { return }
                        guard hosted.documentGeneration == generation else { return }
                        id.withCString { _ = jadix_native_web_ready(self.root, $0, ready) }
                    }
                }
                hosted.update(node["specification"] as? [String: Any] ?? [:])
            }
            if let image = view as? JadixImageView {
                let source = node["source"] as? String
                let style = node["style"] as? [String: Any]
                image.onLoadEvent = { [weak self, weak image] event in
                    guard let self, let image else { return }
                    self.emit(image, event)
                }
                if source?.hasPrefix(applePNGPrefix) == false,
                   style?["width"] as? Double == nil || style?["height"] as? Double == nil {
                    image.onOriginalSizeChange = { [weak self] in self?.scheduleImageLayout() }
                } else {
                    image.onOriginalSizeChange = nil
                }
                image.setSource(source, style: style)
            }
            if let scroll = view as? JadixScrollView, let document = scroll.documentView {
                let style = node["style"] as? [String: Any]
                scroll.onZoom = { [weak self, weak scroll] in
                    guard let self, let scroll else { return }
                    self.refreshBackingDensity(in: scroll)
                    self.updatePopoverPositions()
                    self.scheduleScrollOffsetReport(scroll, id: id)
                }
                scroll.configureZoom(style?["scroll_zoom"] as? [String: Any])
                if let scrollbars = style?["scrollbars"] as? [Bool], scrollbars.count == 2 {
                    scroll.hasHorizontalScroller = scrollbars[0]
                    scroll.hasVerticalScroller = scrollbars[1]
                }
                let inset = style?["scrollbar_inset"] as? [Double] ?? [0, 0, 0, 0]
                scroll.scrollerInsets = NSEdgeInsets(top: inset[0], left: inset[3], bottom: inset[2], right: inset[1])
                let bounces = style?["scroll_bounces"] as? Bool
                let elasticity = bounces.map { $0 ? NSScrollView.Elasticity.allowed : .none } ?? .automatic
                scroll.horizontalScrollElasticity = elasticity
                scroll.verticalScrollElasticity = elasticity
                let children = childrenByParent[id] ?? []
                let contentWidth = children.compactMap { child in
                    (child["frame"] as? [Double]).map { $0[0] + $0[2] - frame[0] }
                }.max() ?? frame[2]
                let contentHeight = children.compactMap { child in
                    (child["frame"] as? [Double]).map { $0[1] + $0[3] - frame[1] }
                }.max() ?? frame[3]
                document.setFrameSize(NSSize(width: max(frame[2], contentWidth), height: max(frame[3], contentHeight)))
                if let target = style?["scroll_to"] as? [Double], target.count == 2,
                   target != scroll.lastScrollTarget, target.allSatisfy({ $0.isFinite && $0 >= 0 }) {
                    scroll.lastScrollTarget = target
                    let point = NSPoint(x: min(target[0], max(0, document.frame.width - scroll.contentView.bounds.width)),
                                        y: min(target[1], max(0, document.frame.height - scroll.contentView.bounds.height)))
                    scroll.scroll(to: point, smoothly: style?["scroll_behavior"] as? String == "smooth")
                } else if style?["scroll_to"] == nil {
                    scroll.lastScrollTarget = nil
                }
                scroll.contentView.postsBoundsChangedNotifications = true
                if scrollObservers[id] == nil {
                    scroll.lastReportedOffset = scroll.contentView.bounds.origin
                    scrollObservers[id] = NotificationCenter.default.addObserver(
                        forName: NSView.boundsDidChangeNotification,
                        object: scroll.contentView,
                        queue: .main
                    ) { [weak self, weak scroll] _ in
                        guard let self, let scroll else { return }
                        if !self.publishing { self.updatePopoverPositions() }
                        self.scheduleScrollOffsetReport(scroll, id: id)
                    }
                }
                scheduleScrollOffsetReport(scroll, id: id)
            }
            if let revision = node["editRevision"] as? String, let value = UInt64(revision) {
                revisions[id] = max(revisions[id, default: 0], value)
            }
            descriptors[id] = node
        }
        for node in nodes {
            guard let id = node["id"] as? String,
                  let view = views[id] else { continue }
            (view as? JadixHostedView)?.needsLayout = true
            let relations = node["accessibilityRelations"] as? [String: Any]
            let targets = relations?["controls"] as? [[String: String]] ?? []
            let linkedViews = targets.compactMap { target -> NSView? in
                guard target["root"] == token,
                      let targetID = target["id"], live.contains(targetID) else { return nil }
                return views[targetID]
            }
            view.setAccessibilityLinkedUIElements(linkedViews)
        }
        var activeDescendants: [String: String] = [:]
        var changedActiveDescendants: [String] = []
        for node in nodes {
            guard let id = node["id"] as? String,
                  let view = views[id],
                  let relations = node["accessibilityRelations"] as? [String: Any],
                  let target = relations["activeDescendant"] as? [String: String],
                  target["root"] == token,
                  let targetID = target["id"], live.contains(targetID),
                  views[targetID] != nil else { continue }
            activeDescendants[id] = targetID
            if accessibilityActiveDescendants[id] != targetID {
                changedActiveDescendants.append(id)
            }
        }
        var selectedChildren: [String: [NSView]] = [:]
        for node in nodes where node["selected"] as? Bool == true {
            guard let parentID = node["parent"] as? String,
                  let id = node["id"] as? String,
                  let child = views[id] else { continue }
            selectedChildren[parentID, default: []].append(child)
            // Virtual rows and spacers are layout containers, not selection owners.
            if node["role"] as? String == "option" {
                var ancestor: String? = parentID
                while let current = ancestor, let descriptor = descriptors[current] {
                    if descriptor["role"] as? String == "listbox" {
                        if current != parentID { selectedChildren[current, default: []].append(child) }
                        break
                    }
                    ancestor = descriptor["parent"] as? String
                }
            }
        }
        for node in nodes {
            guard let id = node["id"] as? String,
                  let view = views[id] else { continue }
            let role = node["role"] as? String
            let supportsSelectedChildren = role.map { ["radiogroup", "tablist", "tree", "grid", "listbox"].contains($0) } ?? false
            let activeChild = activeDescendants[id].flatMap { views[$0] }
            let selected: [Any]?
            if let activeChild {
                selected = [activeChild]
            } else if supportsSelectedChildren {
                selected = selectedChildren[id] ?? []
            } else {
                selected = nil
            }
            view.setAccessibilitySelectedChildren(selected)
        }
        for id in changedActiveDescendants {
            guard let view = views[id] else { continue }
            if let control = view as? NSControl, let cell = control.cell {
                NSAccessibility.post(element: cell, notification: .selectedChildrenChanged)
            } else {
                NSAccessibility.post(element: view, notification: .selectedChildrenChanged)
            }
        }
        accessibilityActiveDescendants = activeDescendants
        updateOverlays(nodes, previousResponder: previousResponder)
        deliverPopoverEvents(removedOpenPopovers)
        updateAccessibilityGrids(nodes)
        updateDialogKeyViewLoop()
        if let previousPopover, previousPopover !== outsideDismissedPopover,
           !previousPopover.isOpen || previousPopover.superview == nil,
           let anchor = previousPopover.anchor, let window, anchor.window === window,
           !anchor.isHiddenOrHasHiddenAncestor, canTakeFocus(anchor),
           isInActiveDialog(anchor) {
            _ = moveFocus(to: anchor)
            if focusChanged, focusedID != nil { applyRequestedFocus() }
        } else if focusChanged { applyRequestedFocus() }
        for pending in preboundViews.values { recycle(pending.view, key: pending.key) }
        preboundViews.removeAll()
        appliedAppearanceRevision = snapshotAppearanceRevision
    }

    private func updateOverlays(_ nodes: [[String: Any]], previousResponder: NSView?) {
        let nextDialogID = nodes.last(where: {
            $0["kind"] as? String == "dialog" && $0["open"] as? Bool == true
        })?["id"] as? String
        let nodesByID = Dictionary(uniqueKeysWithValues: nodes.compactMap { node in
            (node["id"] as? String).map { ($0, node) }
        })
        var openDialogIDs = Set<String>()
        var announcements: [(JadixOverlayView, String)] = []
        var overlays: [JadixOverlayView] = []
        for node in nodes {
            guard let id = node["id"] as? String,
                  let overlay = views[id] as? JadixOverlayView,
                  let frame = node["frame"] as? [Double], frame.count == 4 else { continue }
            let kind = node["kind"] as? String
            overlay.isOpen = node["open"] as? Bool ?? false
            overlay.isModal = kind == "dialog"
            if overlay.isModal { overlay.isHidden = !overlay.isOpen }
            overlay.setAccessibilityRole(overlay.isModal ? .group : .popover)
            overlay.setAccessibilitySubrole(overlay.isModal ? .dialog : nil)
            overlay.setAccessibilityModal(id == nextDialogID)
            if kind == "dialog", overlay.isOpen {
                openDialogIDs.insert(id)
                if let announcement = dialogAnnouncement(id, nodes: nodes, nodesByID: nodesByID),
                   dialogAnnouncementContent[id] != announcement {
                    dialogAnnouncementContent[id] = announcement
                    announcements.append((overlay, announcement))
                }
            }
            overlay.contentHost.wantsLayer = true
            if let dialog = overlay as? JadixDialogView {
                dialog.backdropAlpha = NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency ? 0.85 : 0.32
            }
            let contrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
            overlay.contentHost.layer?.borderWidth = contrast ? 1 : 0
            if overlay.isModal {
                overlay.panelFrame = NSRect(x: frame[0], y: frame[1], width: frame[2], height: frame[3])
                overlay.contentHost.frame = overlay.panelFrame
            }
            overlay.frame = bounds
            overlay.layer?.backgroundColor = NSColor.clear.cgColor
            let style = node["style"] as? [String: Any] ?? [:]
            let fallback: NSColor = overlay.isModal ? .windowBackgroundColor : .clear
            let authoredBackground = color(style["background"], fallback: .clear)
            let background = authoredBackground.alphaComponent == 0 ? fallback : authoredBackground
            effectiveAppearance.performAsCurrentDrawingAppearance {
                overlay.contentHost.layer?.borderColor = contrast ? NSColor.labelColor.cgColor : nil
                if let popover = overlay as? JadixPopoverView {
                    popover.appearanceBackground = background
                    popover.visibleAppearance.background = JadixVisualStyle.color(popover.appearanceWire["background"]) ?? background
                } else {
                    overlay.contentHost.layer?.backgroundColor = background.cgColor
                }
            }
            overlays.append(overlay)
        }
        if !subviews.suffix(overlays.count).elementsEqual(overlays, by: { $0 === $1 }) {
            for overlay in overlays { addSubview(overlay, positioned: .above, relativeTo: nil) }
        }
        for id in Array(dialogAnnouncementContent.keys) where !openDialogIDs.contains(id) {
            dialogAnnouncementContent.removeValue(forKey: id)
        }
        let oldDialogID = activeDialogID
        activeDialogID = nextDialogID
        refreshBackingDensity(in: self)
        updatePopoverPositions()
        if nextDialogID != nil, oldDialogID == nil {
            previousFocus = previousResponder
            dialogFocusWindow = window
        }
        if let nextDialogID, let dialog = views[nextDialogID] as? JadixDialogView,
           nextDialogID != oldDialogID || focusedView().map(isInActiveDialog) != true {
            if !focusFirst(in: dialog.contentHost) { _ = window?.makeFirstResponder(dialog) }
        } else if nextDialogID == nil, oldDialogID != nil {
            if let previousFocus, let window, previousFocus.window === window,
               dialogFocusWindow === window, !previousFocus.isHiddenOrHasHiddenAncestor {
                _ = moveFocus(to: previousFocus)
            }
            previousFocus = nil
            dialogFocusWindow = nil
        }
        for (overlay, announcement) in announcements {
            NSAccessibility.post(
                element: overlay,
                notification: .announcementRequested,
                userInfo: [
                    .announcement: announcement,
                    .priority: NSAccessibilityPriorityLevel.high.rawValue,
                ])
        }
    }

    private func updatePopoverPositions() {
        let safeViewport = bounds.intersection(safeAreaRect)
        let popovers = subviews.compactMap { $0 as? JadixPopoverView }.filter(\.isOpen)
        var positions: [String: JadixPopoverEvent.Position] = [:]
        for popover in popovers {
            popover.anchor = nodeID(for: popover).flatMap { descriptors[$0]?["anchor"] as? String }
                .flatMap { views[$0] }
        }
        var positioning = Set<ObjectIdentifier>()
        var positioned = Set<ObjectIdentifier>()
        var visibilityChanged = false
        func position(_ popover: JadixPopoverView) {
            let identity = ObjectIdentifier(popover)
            guard !positioned.contains(identity), positioning.insert(identity).inserted else { return }
            let wasHidden = popover.isHidden
            var visible = false
            defer {
                if visible { popover.showPanel() } else { popover.hidePanel() }
                visibilityChanged = visibilityChanged || wasHidden != popover.isHidden
                positioning.remove(identity)
                positioned.insert(identity)
            }
            guard let id = nodeID(for: popover) else { return }
            guard !safeViewport.isEmpty, let node = descriptors[id],
                  let anchor = popover.anchor, isInActiveDialog(anchor),
                  let frame = node["frame"] as? [Double], frame.count == 4 else { return }
            if let parent = containingPopover(anchor, includeClosed: true) {
                guard parent.isOpen, !positioning.contains(ObjectIdentifier(parent)) else { return }
                position(parent)
            }
            guard let window, anchor.window === window, !anchor.isHiddenOrHasHiddenAncestor,
                  !anchor.visibleRect.isEmpty else { return }
            let anchorRect = convert(anchor.bounds, from: anchor)
            guard safeViewport.intersects(convert(anchor.visibleRect, from: anchor)) else { return }

            let style = node["style"] as? [String: Any]
            let margin = CGFloat(style?["popover_margin"] as? Double ?? 0)
            let viewport = safeViewport.insetBy(
                dx: min(margin, safeViewport.width / 2), dy: min(margin, safeViewport.height / 2)
            )
            guard !viewport.isEmpty else { return }

            let gap = CGFloat(node["gap"] as? Double ?? 0)
            let placement = node["placement"] as? String ?? "bottom-start"
            let width = min(CGFloat(frame[2]), viewport.width)
            let height = min(CGFloat(frame[3]), viewport.height)
            var side = placement.split(separator: "-").first.map(String.init) ?? "bottom"
            let above = anchorRect.minY - viewport.minY - gap
            let below = viewport.maxY - anchorRect.maxY - gap
            let left = anchorRect.minX - viewport.minX - gap
            let right = viewport.maxX - anchorRect.maxX - gap
            switch side {
            case "top" where above < height && below > above: side = "bottom"
            case "bottom" where below < height && above > below: side = "top"
            case "left" where left < width && right > left: side = "right"
            case "right" where right < width && left > right: side = "left"
            default: break
            }

            let alignEnd = placement.hasSuffix("-end")
            let alignStart = placement.hasSuffix("-start")
            var x: CGFloat
            var y: CGFloat
            switch side {
            case "left", "right":
                x = side == "left" ? anchorRect.minX - width - gap : anchorRect.maxX + gap
                y = alignStart ? anchorRect.minY : anchorRect.midY - height / 2
                if alignEnd { y = anchorRect.maxY - height }
            default:
                let rtl = isRightToLeft(from: nodeID(for: anchor))
                let alignRight = (alignEnd && !rtl) || (alignStart && rtl)
                x = alignStart || alignEnd ? anchorRect.minX : anchorRect.midX - width / 2
                if alignRight { x = anchorRect.maxX - width }
                y = side == "top" ? anchorRect.minY - height - gap : anchorRect.maxY + gap
            }
            x = max(viewport.minX, min(x, viewport.maxX - width))
            y = max(viewport.minY, min(y, viewport.maxY - height))
            let rootFrame = NSRect(x: x, y: y, width: width, height: height)
            popover.panelFrame = popover.convert(rootFrame, from: self)
            if popover.contentHost.frame != popover.panelFrame { popover.contentHost.frame = popover.panelFrame }
            popover.updateAppearance()
            let availableSize: NSSize
            switch side {
            case "top": availableSize = NSSize(width: viewport.width, height: max(0, above))
            case "left": availableSize = NSSize(width: max(0, left), height: viewport.height)
            case "right": availableSize = NSSize(width: max(0, right), height: viewport.height)
            default: availableSize = NSSize(width: viewport.width, height: max(0, below))
            }
            let alignment = alignStart ? "-start" : (alignEnd ? "-end" : "")
            positions[id] = .init(frame: rootFrame, placement: side + alignment, availableSize: availableSize)
            visible = true
        }
        for popover in popovers { position(popover) }
        let openIDs = popovers.compactMap { nodeID(for: $0) }
        let currentOpen = Set(openIDs)
        var events: [(String, JadixPopoverEvent)] = []
        for id in openPopoverIDs where !currentOpen.contains(id) {
            guard let popover = views[id] as? JadixPopoverView else { continue }
            popover.reportedPosition = nil
            if popover.closePanel(completion: { [weak self, weak popover] in
                guard let self, let popover, self.views[id] === popover else { return }
                self.deliverPopoverEvents([(id, .closed)])
                self.updateWindowEventMonitor()
            }) {
                continue
            }
            events.append((id, .closed))
        }
        for popover in popovers {
            guard let id = nodeID(for: popover) else { continue }
            if !popover.reportedOpen, !popover.isHidden { events.append((id, .opened)) }
            if positions[id] != popoverPositions[id] { events.append((id, .positionChanged(positions[id]))) }
        }
        openPopoverIDs = openIDs
        popoverPositions = positions
        deliverPopoverEvents(events)
        updateWindowEventMonitor()
        if visibilityChanged && !publishing { updateDialogKeyViewLoop() }
    }

    private func deliverPopoverEvents(_ events: [(String, JadixPopoverEvent)]) {
        guard !events.isEmpty else { return }
        let generation = popoverEventGeneration
        let eventViews = events.map { views[$0.0] as? JadixPopoverView }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            for ((id, event), view) in zip(events, eventViews) {
                guard self.root != 0, self.popoverEventGeneration == generation else { return }
                switch event {
                case .opened, .closed:
                    if view == nil, case .closed = event {
                        self.onPopoverEvent?(id, event)
                        continue
                    }
                    guard let view, self.views[id] === view else { continue }
                    let open: Bool
                    if case .opened = event { open = true } else { open = false }
                    guard view.isOpen == open, view.reportedOpen != open,
                          open ? !view.isHidden : !view.isClosing else { continue }
                    view.reportedOpen = open
                    self.emit(view, ["kind": "popoverLifecycle", "open": open], node: id)
                    guard self.root != 0, self.popoverEventGeneration == generation else { return }
                    self.onPopoverEvent?(id, event)
                case .positionChanged(let position):
                    self.onPopoverEvent?(id, event)
                    guard self.popoverEventGeneration == generation else { return }
                    guard let view, self.views[id] === view else { continue }
                    self.reportPopoverPosition(position, view: view, id: id)
                case .dismissRequested:
                    break
                }
            }
        }
    }

    private func reportPopoverPosition(
        _ position: JadixPopoverEvent.Position?, view: JadixPopoverView, id: String
    ) {
        guard view.isOpen, position == popoverPositions[id], position != view.reportedPosition else { return }
        view.reportedPosition = position
        var payload: [String: Any] = ["kind": "popoverPosition"]
        if let position {
            let frame = position.frame
            payload["position"] = [
                "frame": ["x": frame.minX, "y": frame.minY, "width": frame.width, "height": frame.height],
                "placement": position.placement,
                "availableWidth": position.availableSize.width,
                "availableHeight": position.availableSize.height,
            ] as [String: Any]
        }
        emit(view, payload, node: id)
    }

    private func focusedView() -> NSView? {
        guard let responder = window?.firstResponder as? NSView else { return nil }
        return views.values.first { ($0 as? NSTextField)?.currentEditor() === responder } ?? responder
    }

    private func containingPopover(_ view: NSView, includeClosed: Bool = false) -> JadixPopoverView? {
        var ancestor = view.superview
        while let current = ancestor, current !== self {
            if let popover = current as? JadixPopoverView, includeClosed || popover.isOpen { return popover }
            ancestor = current.superview
        }
        return nil
    }

    private func isInActiveDialog(_ view: NSView) -> Bool {
        guard let activeDialogID, let dialog = views[activeDialogID] as? JadixDialogView else { return true }
        var current = view
        var visited = Set<ObjectIdentifier>()
        while visited.insert(ObjectIdentifier(current)).inserted {
            if current === dialog || current.isDescendant(of: dialog.contentHost) { return true }
            guard let anchor = containingPopover(current)?.anchor else { return false }
            current = anchor
        }
        return false
    }

    fileprivate func updateWindowEventMonitor() {
        let hasOverlay = subviews.contains { $0 is JadixOverlayView && !$0.isHidden }
        let hasDragRegion = descriptors.values.contains {
            ($0["style"] as? [String: Any])?["window_drag"] as? Bool == true
        }
        let hasNavigation = descriptors.values.contains {
            !(($0["navigationKeys"] as? [String]) ?? []).isEmpty
        }
        if window == nil || (!hasOverlay && scrollObservers.isEmpty && !hasDragRegion
                             && !hasNavigation && window.map(hasHiddenTitleBar) != true) {
            if let windowEventMonitor { NSEvent.removeMonitor(windowEventMonitor) }
            windowEventMonitor = nil
            return
        }
        guard windowEventMonitor == nil else { return }
        let mask: NSEvent.EventTypeMask = [.leftMouseDown, .rightMouseDown, .otherMouseDown, .keyDown]
        windowEventMonitor = NSEvent.addLocalMonitorForEvents(matching: mask) { [weak self] event in
            guard let self, let window = self.window, event.window === window else { return event }
            if event.type == .keyDown,
               event.modifierFlags.intersection([.shift, .control, .option, .command]).isEmpty,
               (window.firstResponder as? NSTextInputClient)?.hasMarkedText() != true,
               let view = self.focusedView(), let id = self.nodeID(for: view),
               let descriptor = self.descriptors[id], descriptor["enabled"] as? Bool != false,
               let key = Self.navigationKey(event.keyCode),
               (descriptor["navigationKeys"] as? [String])?.contains(key) == true {
                self.emit(view, ["kind": "navigate", "key": key, "shift": false, "control": false])
                return nil
            }
            if event.type == .keyDown, event.keyCode == 48, !self.scrollObservers.isEmpty {
                let previous = self.focusedView()
                // Let AppKit resolve the responder and honor the active key-view loop first.
                DispatchQueue.main.async { [weak self, weak window, weak previous] in
                    guard let self, let window, self.window === window,
                          let view = self.focusedView(), view !== previous,
                          view.isDescendant(of: self) else { return }
                    self.revealFocusedView(view)
                }
                return event
            }
            guard let overlay = self.subviews.reversed().compactMap({ $0 as? JadixOverlayView })
                    .first(where: { $0.isOpen && !$0.isHidden }) else {
                return self.dragWindowIfRequested(window, event: event) ? nil : event
            }
            if event.type == .keyDown {
                guard event.keyCode == 53,
                      (window.firstResponder as? NSTextInputClient)?.hasMarkedText() != true else { return event }
                if let popover = overlay as? JadixPopoverView {
                    let allowed = popover.closeOnEscape
                    self.reportPopoverCancel(popover, backdrop: false, allowed: allowed)
                    guard allowed else { return event }
                    guard popover.isOpen, !popover.isHidden, popover.closeOnEscape,
                          self.nodeID(for: popover) != nil else { return nil }
                }
                overlay.onDismiss?(false)
                return nil
            }
            guard let popover = overlay as? JadixPopoverView else { return event }
            if let anchor = popover.anchor,
               anchor.visibleRect.contains(anchor.convert(event.locationInWindow, from: nil)) {
                return event
            }
            let point = popover.convert(event.locationInWindow, from: nil)
            guard !popover.panelFrame.contains(point) else { return event }
            let allowed = popover.closeOnOutside
            self.outsideDismissedPopover = popover
            defer { self.outsideDismissedPopover = nil }
            self.reportPopoverCancel(popover, backdrop: true, allowed: allowed)
            guard allowed, popover.isOpen, !popover.isHidden, popover.closeOnOutside,
                  self.nodeID(for: popover) != nil else { return event }
            popover.onDismiss?(true)
            return event
        }
    }

    private func reportPopoverCancel(_ popover: JadixPopoverView, backdrop: Bool, allowed: Bool) {
        guard let id = nodeID(for: popover) else { return }
        deliverPopoverEvents([(id, .dismissRequested(backdrop ? .outsidePress : .escape, allowed: allowed))])
        emit(popover, ["kind": "popoverCancel", "backdrop": backdrop], node: id)
    }

    private static func navigationKey(_ code: UInt16) -> String? {
        switch code {
        case 123: return "left"
        case 124: return "right"
        case 125: return "down"
        case 126: return "up"
        case 115: return "home"
        case 119: return "end"
        case 116: return "pageUp"
        case 121: return "pageDown"
        case 36, 76: return "enter"
        case 53: return "escape"
        default: return nil
        }
    }

    private func dragWindowIfRequested(_ window: NSWindow, event: NSEvent) -> Bool {
        guard event.type == .leftMouseDown, window.isMovable,
              !window.styleMask.contains(.fullScreen), window.attachedSheet == nil else { return false }
        let windowView = window.contentView?.superview ?? self
        let point = windowView.superview?.convert(event.locationInWindow, from: nil) ?? event.locationInWindow
        guard let hit = windowView.hitTest(point), hit.isDescendant(of: self) else { return false }
        var candidate: NSView? = hit
        var enabled: Bool?
        while let view = candidate, view !== self {
            let node = nodeID(for: view).flatMap { descriptors[$0] }
            // Static text uses an NSTextField, just like editable fields.
            if view is NSTextView || (view is NSControl && node?["kind"] as? String != "text") {
                return false
            }
            if let style = node?["style"] as? [String: Any],
               let value = style["window_drag"] as? Bool {
                enabled = value
                break
            }
            candidate = view.superview
        }
        let inTitleBar = hasHiddenTitleBar(window) && event.locationInWindow.y >= window.contentLayoutRect.maxY
        guard enabled ?? inTitleBar else { return false }
        if event.clickCount == 2 {
            switch UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") {
            case "Minimize": window.miniaturize(nil)
            case "None": break
            default: window.performZoom(nil)
            }
        } else {
            window.performDrag(with: event)
        }
        return true
    }

    private func hasHiddenTitleBar(_ window: NSWindow) -> Bool {
        window.titleVisibility == .hidden && window.titlebarAppearsTransparent
            && window.styleMask.contains([.titled, .fullSizeContentView])
    }

    private func dialogAnnouncement(
        _ dialogID: String,
        nodes: [[String: Any]],
        nodesByID: [String: [String: Any]]
    ) -> String? {
        var parts: [String] = []
        for node in nodes {
            guard let id = node["id"] as? String else { continue }
            var ancestor = node["parent"] as? String
            var visited = Set<String>()
            var belongsToDialog = id == dialogID
            while let parentID = ancestor, visited.insert(parentID).inserted {
                if parentID == dialogID {
                    belongsToDialog = true
                    break
                }
                ancestor = nodesByID[parentID]?["parent"] as? String
            }
            guard belongsToDialog else { continue }

            let kind = node["kind"] as? String
            var values = [node["name"] as? String, node["description"] as? String]
            if id == dialogID || kind == "text" || kind == "button" {
                values.append(node["text"] as? String)
            }
            for value in values.compactMap({ $0?.trimmingCharacters(in: .whitespacesAndNewlines) })
                where !value.isEmpty && !parts.contains(value) {
                parts.append(value)
            }
        }
        let announcement = parts.joined(separator: ". ")
        return announcement.isEmpty ? nil : announcement
    }

    private func updateAccessibilityGrids(_ nodes: [[String: Any]]) {
        var nextGridIDs = Set<String>()
        var cellHeaders: [String: [NSView]] = [:]
        var gridMembers: [String: [[String: Any]]] = [:]
        for node in nodes {
            guard node["cell"] is [String: Int] || node["columnHeader"] is [String: Int] else { continue }
            guard let id = node["id"] as? String, let gridID = nearestGridID(for: id) else { continue }
            gridMembers[gridID, default: []].append(node)
        }
        for node in nodes where node["role"] as? String == "grid" {
            guard let gridID = node["id"] as? String, let grid = views[gridID] else { continue }
            nextGridIDs.insert(gridID)
            var rows: [Int: [(column: Int, view: NSView)]] = [:]
            var headers: [JadixAccessibilityGridHeader] = []
            var indexedCells: [JadixAccessibilityGridCell] = []
            var cellColumns: [String: Range<Int>] = [:]
            var columnAnchors = Set<Int>()
            let members = gridMembers[gridID] ?? []
            for cellNode in members {
                guard let cellID = cellNode["id"] as? String,
                      let cell = views[cellID] else { continue }
                if let coordinates = cellNode["cell"] as? [String: Int],
                   let row = coordinates["row"],
                   let column = coordinates["column"] {
                    let rowSpan = max(1, coordinates["rowSpan"] ?? 1)
                    let columnSpan = max(1, coordinates["columnSpan"] ?? 1)
                    rows[row, default: []].append((column, cell))
                    let coveredColumns = column..<(column + columnSpan)
                    indexedCells.append(.init(rows: row..<(row + rowSpan), columns: coveredColumns, view: cell))
                    cellColumns[cellID] = coveredColumns
                    columnAnchors.insert(column)
                } else if let header = cellNode["columnHeader"] as? [String: Int],
                          let column = header["column"] {
                    rows[0, default: []].append((column, cell))
                    let columnSpan = max(1, header["columnSpan"] ?? 1)
                    headers.append(.init(columns: column..<(column + columnSpan), view: cell))
                    columnAnchors.insert(column)
                }
            }
            headers.sort { $0.columns.lowerBound < $1.columns.lowerBound }
            indexedCells.sort {
                ($0.rows.lowerBound, $0.columns.lowerBound) < ($1.rows.lowerBound, $1.columns.lowerBound)
            }
            if let container = grid as? JadixContainerView {
                container.accessibilityGridCells = indexedCells
                container.accessibilityGridHeaders = headers
            }

            var rowElements: [JadixAccessibilityRow] = []
            var visibleRowElements: [JadixAccessibilityRow] = []
            var visibleCellViews: [NSView] = []
            var nextRows: [Int: JadixAccessibilityRow] = [:]
            for index in rows.keys.sorted() {
                guard let cells = rows[index]?.sorted(by: { $0.column < $1.column }) else { continue }
                let cellViews = cells.map(\.view)
                let visibleCells = cellViews.filter(\.isVisibleToGridAccessibility)
                let row = accessibilityGridRows[gridID]?[index] ?? JadixAccessibilityRow()
                row.update(parent: grid, index: index, cells: cellViews)
                nextRows[index] = row
                rowElements.append(row)
                visibleCellViews.append(contentsOf: visibleCells)
                if !visibleCells.isEmpty { visibleRowElements.append(row) }
            }
            for (index, row) in accessibilityGridRows[gridID] ?? [:] where nextRows[index] == nil {
                row.clear()
            }
            accessibilityGridRows[gridID] = nextRows
            grid.setAccessibilityRows(rowElements)
            grid.setAccessibilityVisibleRows(visibleRowElements)
            grid.setAccessibilityVisibleCells(visibleCellViews)
            grid.setAccessibilityChildren(rowElements)
            grid.setAccessibilityHeader(headers.isEmpty ? nil : nextRows[0])
            grid.setAccessibilityColumnHeaderUIElements(headers.map(\.view))

            // Materialize columns anchored by mounted children. The grid still reports logical
            // dimensions and resolves cells at any covered coordinate via accessibilityCell.
            let sortedAnchors = columnAnchors.sorted()
            var columns: [Int: [NSView]] = [:]
            func addColumnChildren(_ span: Range<Int>, view: NSView) {
                var lower = 0
                var upper = sortedAnchors.count
                while lower < upper {
                    let middle = lower + (upper - lower) / 2
                    if sortedAnchors[middle] < span.lowerBound {
                        lower = middle + 1
                    } else {
                        upper = middle
                    }
                }
                for index in sortedAnchors[lower...] {
                    if index >= span.upperBound { break }
                    columns[index, default: []].append(view)
                }
            }
            for header in headers { addColumnChildren(header.columns, view: header.view) }
            for cell in indexedCells { addColumnChildren(cell.columns, view: cell.view) }
            var columnElements: [NSAccessibilityElement] = []
            var visibleColumnElements: [NSAccessibilityElement] = []
            var nextColumns: [Int: JadixAccessibilityColumn] = [:]
            var headerIndex = 0
            for index in sortedAnchors {
                guard let cells = columns[index] else { continue }
                let column = accessibilityGridColumns[gridID]?[index] ?? JadixAccessibilityColumn()
                column.cells = cells
                column.setAccessibilityElement(true)
                column.setAccessibilityRole(.column)
                column.setAccessibilityIndex(index)
                column.setAccessibilityParent(grid)
                column.setAccessibilityChildren(cells)
                while headerIndex < headers.count && headers[headerIndex].columns.upperBound <= index {
                    headerIndex += 1
                }
                let header = headerIndex < headers.count && headers[headerIndex].columns.contains(index)
                    ? headers[headerIndex].view : nil
                column.setAccessibilityHeader(header)
                let frame = cells.reduce(CGRect.null) { frame, cell in
                    frame.union(grid.convert(cell.bounds, from: cell))
                }
                column.setAccessibilityFrameInParentSpace(frame)
                nextColumns[index] = column
                columnElements.append(column)
                if column.hasVisibleCells { visibleColumnElements.append(column) }
            }
            for (index, column) in accessibilityGridColumns[gridID] ?? [:] where nextColumns[index] == nil {
                clearAccessibilityColumn(column)
            }
            accessibilityGridColumns[gridID] = nextColumns
            grid.setAccessibilityColumns(columnElements)
            grid.setAccessibilityVisibleColumns(visibleColumnElements)
            let dimensions = node["grid"] as? [String: Int] ?? [:]
            (grid as? JadixContainerView)?.setGridAxes(rows: nextRows, columns: nextColumns,
                rowCount: dimensions["rows"] ?? 0, columnCount: dimensions["columns"] ?? 0)

            for cellNode in members where cellNode["role"] as? String == "gridcell" {
                guard let cellID = cellNode["id"] as? String,
                      let span = cellColumns[cellID] else { continue }
                let matchingHeaders = headers.filter { $0.columns.overlaps(span) }.map(\.view)
                if !matchingHeaders.isEmpty { cellHeaders[cellID] = matchingHeaders }
            }
        }
        for cellNode in nodes where cellNode["role"] as? String == "gridcell" {
            guard let id = cellNode["id"] as? String, let view = views[id] else { continue }
            view.setAccessibilityColumnHeaderUIElements(cellHeaders[id])
        }
        for gridID in Array(accessibilityGridRows.keys) where !nextGridIDs.contains(gridID) {
            if let rows = accessibilityGridRows.removeValue(forKey: gridID) {
                for row in rows.values { row.clear() }
            }
            if let columns = accessibilityGridColumns.removeValue(forKey: gridID) {
                for column in columns.values { clearAccessibilityColumn(column) }
            }
            if let grid = views[gridID] {
                (grid as? JadixContainerView)?.clearAccessibilityGrid()
                grid.setAccessibilityRows(nil)
                grid.setAccessibilityVisibleRows(nil)
                grid.setAccessibilityVisibleCells(nil)
                grid.setAccessibilityChildren(nil)
                grid.setAccessibilityColumnHeaderUIElements(nil)
                grid.setAccessibilityColumns(nil)
                grid.setAccessibilityVisibleColumns(nil)
            }
        }
    }

    private func clearAccessibilityColumn(_ column: JadixAccessibilityColumn) {
        column.cells.removeAll()
        column.setAccessibilityChildren(nil)
        column.setAccessibilityHeader(nil)
        column.setAccessibilityParent(nil)
    }

    private func nearestGridID(for id: String) -> String? {
        var parent = descriptors[id]?["parent"] as? String
        while let current = parent, let descriptor = descriptors[current] {
            if descriptor["role"] as? String == "grid" { return current }
            parent = descriptor["parent"] as? String
        }
        return nil
    }

    private func focusFirst(in view: NSView) -> Bool {
        if canTakeFocus(view), moveFocus(to: view) { return true }
        for child in view.subviews where !child.isHidden {
            if focusFirst(in: child) { return true }
        }
        return false
    }

    private func canTakeFocus(_ view: NSView) -> Bool {
        guard !view.isHiddenOrHasHiddenAncestor, view.acceptsFirstResponder else { return false }
        if containingPopover(view, includeClosed: true)?.isOpen == false { return false }
        if let control = view as? NSControl { return control.isEnabled }
        if let text = view as? NSTextView { return text.isEditable && text.isSelectable }
        return view is JadixDialogView
    }

    private func moveFocus(to view: NSView) -> Bool {
        guard let window, view.window === window, window.makeFirstResponder(view) else { return false }
        if publishing {
            // Scroll events can publish a new snapshot through the Rust host.
            DispatchQueue.main.async { [weak self, weak view] in
                guard let self, let view, self.focusedView() === view else { return }
                self.revealFocusedView(view)
            }
        } else {
            revealFocusedView(view)
        }
        return true
    }

    private func revealFocusedView(_ view: NSView) {
        guard let window, view.window === window, !view.isHiddenOrHasHiddenAncestor else { return }
        let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        var targets: [(NSScrollView, NSPoint)] = []
        var projectedFrame = convert(view.bounds, from: view)
        var enclosing = view.enclosingScrollView
        while let scroll = enclosing {
            enclosing = scroll.enclosingScrollView
            let clip = scroll.contentView
            let bounds = clip.bounds
            guard scroll.documentView != nil, !bounds.isEmpty else { continue }
            let frame = clip.convert(projectedFrame, from: self)
            var proposed = bounds
            proposed.origin.x += focusRevealOffset(
                start: frame.minX - bounds.minX, end: frame.maxX - bounds.minX, viewport: bounds.width
            )
            proposed.origin.y += focusRevealOffset(
                start: frame.minY - bounds.minY, end: frame.maxY - bounds.minY, viewport: bounds.height
            )
            let destination = clip.constrainBoundsRect(proposed)
            let deltaX = destination.minX - bounds.minX
            let deltaY = destination.minY - bounds.minY
            if abs(deltaX) >= 0.5 || abs(deltaY) >= 0.5 {
                targets.append((scroll, destination.origin))
                // Project the inner scroll's destination before computing the outer one.
                projectedFrame = convert(frame.offsetBy(dx: -deltaX, dy: -deltaY), from: clip)
            }
        }
        guard !targets.isEmpty else { return }
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = reduceMotion ? 0 : 0.3
            for (scroll, destination) in targets {
                let clip = scroll.contentView
                if reduceMotion {
                    clip.scroll(to: destination)
                } else {
                    clip.animator().setBoundsOrigin(destination)
                }
                scroll.reflectScrolledClipView(clip)
            }
        }, completionHandler: nil)
        updatePopoverPositions()
    }

    private func focusRevealOffset(start: CGFloat, end: CGFloat, viewport: CGFloat) -> CGFloat {
        if end - start > viewport, end > 0, start < viewport { return 0 }
        let margin = min(16, viewport / 4)
        if start < margin { return start - margin }
        if end > viewport - margin { return min(end - (viewport - margin), start - margin) }
        return 0
    }

    private func updateDialogKeyViewLoop() {
        let dialog = activeDialogID.flatMap { views[$0] as? JadixOverlayView }
        let dialogID = dialog?.isOpen == true ? activeDialogID : nil
        if keyViewLoopDialogID != dialogID {
            restoreKeyViewLinks()
            keyViewLoopDialogID = dialogID
        }

        let focusable = nodeOrder.compactMap { id -> NSView? in
            guard let node = descriptors[id],
                  let view = views[id],
                  !(view is JadixOverlayView), isInActiveDialog(view),
                  canTakeFocus(view) else { return nil }
            if node["tabStop"] as? Bool == false { return nil }
            return view
        }
        var loop = focusable
        if loop.isEmpty, dialogID != nil, let dialog {
            loop.append(dialog)
            if window?.firstResponder !== dialog {
                _ = window?.makeFirstResponder(dialog)
            }
        }
        let nextIDs = Set(loop.map { ObjectIdentifier($0) })
        let staleIDs = savedKeyViewLinks.keys.filter { !nextIDs.contains($0) }
        for id in staleIDs {
            guard let (view, next) = savedKeyViewLinks[id] else { continue }
            view.nextKeyView = next
            savedKeyViewLinks.removeValue(forKey: id)
        }
        for (index, view) in loop.enumerated() {
            let next = loop[(index + 1) % loop.count]
            let id = ObjectIdentifier(view)
            if savedKeyViewLinks[id] == nil {
                savedKeyViewLinks[id] = (view, view.nextKeyView)
            }
            view.nextKeyView = next
        }
    }

    private func moveRadio(_ button: JadixRadioButton, keyCode: UInt16) -> Bool {
        guard let id = nodeID(for: button),
              let descriptor = descriptors[id],
              descriptor["role"] as? String == "radio",
              let groupID = radioGroupID(for: id) else { return false }
        var offset = (keyCode == 123 || keyCode == 126) ? -1 : 1
        if keyCode == 123 || keyCode == 124,
           isRightToLeft(from: groupID) { offset *= -1 }
        let candidates = nodeOrder.compactMap { candidateID -> (String, JadixRadioButton)? in
            guard radioGroupID(for: candidateID) == groupID,
                  let candidate = views[candidateID] as? JadixRadioButton,
                  isInActiveDialog(candidate), canTakeFocus(candidate) else { return nil }
            return (candidateID, candidate)
        }
        guard candidates.count > 1,
              let currentIndex = candidates.firstIndex(where: { $0.0 == id }) else { return false }
        let targetIndex = (currentIndex + offset + candidates.count) % candidates.count
        let target = candidates[targetIndex]
        guard moveFocus(to: target.1) else { return false }
        emit(target.1, ["kind": "activate"], node: target.0)
        return true
    }

    private func isRightToLeft(from id: String?) -> Bool {
        var nodeID = id
        while let current = nodeID, let node = descriptors[current] {
            let style = node["style"] as? [String: Any]
            if style?["direction"] as? String == "right-to-left" { return true }
            if style?["direction"] as? String == "left-to-right" { return false }
            nodeID = node["parent"] as? String
        }
        return userInterfaceLayoutDirection == .rightToLeft
    }

    private func radioGroupID(for id: String) -> String? {
        var parentID = descriptors[id]?["parent"] as? String
        while let current = parentID {
            guard let descriptor = descriptors[current] else { return nil }
            if descriptor["role"] as? String == "radiogroup" { return current }
            parentID = descriptor["parent"] as? String
        }
        return nil
    }

    private func restoreKeyViewLinks() {
        for (view, next) in savedKeyViewLinks.values { view.nextKeyView = next }
        savedKeyViewLinks.removeAll()
    }

    private func applyRequestedFocus() {
        guard let window else { return }
        if let focusedID, let target = views[focusedID] {
            if let activeDialogID,
               let dialog = views[activeDialogID] as? JadixOverlayView,
               !isInActiveDialog(target) {
                if !focusFirst(in: dialog.contentHost) { _ = window.makeFirstResponder(dialog) }
                return
            }
            if canTakeFocus(target), focusedView() !== target { _ = moveFocus(to: target) }
        } else if activeDialogID == nil, let responder = focusedView(), nodeID(for: responder) != nil {
            _ = window.makeFirstResponder(nil)
        }
    }

    fileprivate func clear() {
        webResourceReplies.removeAll()
        speechService.dispose()
        desktopServices.dispose()
        fetchService.dispose(); fetchService = JadixFetchService()
        audioService.dispose(); audioService = JadixAudioService()
        statusItem.dispose()
        webPump?.invalidate(); webPump = nil
        performanceLink?.invalidate(); performanceLink = nil
        performancePanel.removeFromSuperview()
        performanceWork = 0
        handoff.clear()
        systemShare?.cancel()
        systemShare = nil
        activeContextMenu?.cancelTracking()
        activeContextMenu = nil
        windowMenuBar?.target?.cancelIcons()
        windowMenuBar = nil
        windowMenuRevision = 0
        refreshWindowMenu()
        for observer in windowStateObservers { NotificationCenter.default.removeObserver(observer) }
        windowStateObservers.removeAll()
        pendingWindowStateRoot = nil
        if let windowEventMonitor { NSEvent.removeMonitor(windowEventMonitor) }
        windowEventMonitor = nil
        outsideDismissedPopover = nil
        openPopoverIDs.removeAll()
        popoverPositions.removeAll()
        popoverEventGeneration = UUID()
        for observer in scrollObservers.values { NotificationCenter.default.removeObserver(observer) }
        scrollObservers.removeAll()
        for observer in environmentObservers {
            NSWorkspace.shared.notificationCenter.removeObserver(observer)
            NotificationCenter.default.removeObserver(observer)
        }
        environmentObservers.removeAll()
        for view in views.values {
            (view as? JadixHostedView)?.dispose()
            (view as? JadixContainerView)?.clearAccessibilityGrid()
            (view as? JadixImageView)?.setSource(nil)
            clearTextUndoActions(for: view)
            view.removeFromSuperview()
        }
        remoteImages?.invalidate()
        remoteImages = nil
        for renderer in viewAppearances.values { renderer.remove() }
        viewAppearances.removeAll()
        views.removeAll()
        recycledViews.removeAll(); recycledViewCount = 0
        preboundViews.removeAll()
        nodeIDsByView.removeAll()
        descriptors.removeAll()
        deferredEditorAppearances.removeAll()
        accessibilityActiveDescendants.removeAll()
        for rows in accessibilityGridRows.values {
            for row in rows.values { row.clear() }
        }
        accessibilityGridRows.removeAll()
        for columns in accessibilityGridColumns.values {
            for column in columns.values { clearAccessibilityColumn(column) }
        }
        accessibilityGridColumns.removeAll()
        nodeOrder.removeAll()
        revisions.removeAll()
        focusedID = nil
        previousFocus = nil
        dialogFocusWindow = nil
        observedWindow = nil
        activeDialogID = nil
        dialogAnnouncementContent.removeAll()
        keyViewLoopDialogID = nil
        restoreKeyViewLinks()
    }

    private func nodeID(for view: NSView) -> String? {
        nodeIDsByView[ObjectIdentifier(view)]
    }

    private func refreshBackingDensity(in ancestor: NSView) {
        for (id, view) in views where view === ancestor || view.isDescendant(of: ancestor) {
            viewAppearances[id]?.refreshBackingDensity()
            (view as? JadixImageView)?.refreshBackingDensity()
        }
    }

    private func scheduleScrollOffsetReport(_ scroll: JadixScrollView, id: String) {
        guard !scroll.offsetReportPending,
              (scroll.contentView.bounds.origin != scroll.lastReportedOffset ||
               scroll.contentView.bounds.size != scroll.lastReportedViewport ||
               scroll.magnification != scroll.lastReportedScale) else { return }
        scroll.offsetReportPending = true
        // Native events may synchronously publish a snapshot through the Rust host.
        DispatchQueue.main.async { [weak self, weak scroll] in
            guard let scroll else { return }
            scroll.offsetReportPending = false
            guard let self, self.root != 0, self.views[id] === scroll else { return }
            let offset = scroll.contentView.bounds.origin
            let viewport = scroll.contentView.bounds.size
            let scale = scroll.magnification
            guard offset != scroll.lastReportedOffset || viewport != scroll.lastReportedViewport || scale != scroll.lastReportedScale else { return }
            let zoomChanged = scale != scroll.lastReportedScale
            if zoomChanged { self.refreshBackingDensity(in: scroll) }
            scroll.lastReportedOffset = offset
            scroll.lastReportedViewport = viewport
            scroll.lastReportedScale = scale
            self.emit(scroll, ["kind": "scroll", "x": offset.x, "y": offset.y,
                               "viewport": [viewport.width, viewport.height]], node: id)
            if zoomChanged, self.views[id] === scroll {
                self.emit(scroll, ["kind": "scrollZoom", "scale": scale, "x": offset.x, "y": offset.y,
                                   "width": viewport.width, "height": viewport.height], node: id)
            }
        }
    }

    private func emit(_ view: NSView, _ payload: [String: Any], node: String? = nil) {
        guard root != 0, !publishing, let id = node ?? nodeID(for: view) else { return }
        let event: [String: Any] = ["root": String(root), "node": id, "payload": payload]
        guard let data = try? JSONSerialization.data(withJSONObject: event),
              let text = String(data: data, encoding: .utf8) else { return }
        do {
            let state = try text.withCString { try reply(jadix_native_event(root, $0)) }
            onStateChange?(state)
        } catch { onError?(error) }
    }

    private func edit(_ view: NSView, text: String) {
        guard let id = nodeID(for: view), let revision = revisions[id] else { return }
        let selection: NSRange
        var composing: NSRange?
        if let field = view as? NSTextField, let editor = field.currentEditor() {
            selection = editor.selectedRange
            composing = markedRange(in: editor)
        } else if let textView = view as? NSTextView {
            let client: NSTextInputClient = textView
            selection = client.selectedRange()
            composing = markedRange(in: textView)
        } else {
            selection = NSRange(location: text.utf16.count, length: 0)
        }
        let composingRange: Any = composing.map { [$0.location, NSMaxRange($0)] } ?? NSNull()
        emit(view, ["kind": "edit", "value": text, "revision": String(revision),
                   "selection": [selection.location, NSMaxRange(selection)], "composing": composingRange])
    }

    private func markedRange(in textInput: NSText) -> NSRange? {
        guard let client = textInput as? NSTextInputClient, client.hasMarkedText() else { return nil }
        let range = client.markedRange()
        return range.location == NSNotFound ? nil : range
    }

    @objc private func activate(_ sender: NSButton) { emit(sender, ["kind": "activate"]) }
    @objc private func toggled(_ sender: NSButton) {
        guard let id = nodeID(for: sender), let revision = revisions[id] else { return }
        emit(sender, ["kind": "toggle", "checked": sender.state == .on, "revision": String(revision)])
    }
    @objc private func selected(_ sender: NSPopUpButton) {
        guard let id = nodeID(for: sender), let revision = revisions[id],
              let value = sender.selectedItem?.representedObject as? String else { return }
        emit(sender, ["kind": "select", "value": value, "revision": String(revision)])
    }
    @objc private func submit(_ sender: NSTextField) { emit(sender, ["kind": "submit", "revision": String(revisions[nodeID(for: sender) ?? ""] ?? 0)]) }

    public func controlTextDidChange(_ notification: Notification) {
        guard let field = notification.object as? NSTextField else { return }
        refreshEditorAppearance(field)
        edit(field, text: field.stringValue)
    }

    private func refreshEditorAppearance(_ view: NSView) {
        guard let id = nodeID(for: view), deferredEditorAppearances.contains(id),
              let node = descriptors[id] else { return }
        // A style update deferred during composition takes effect on commit,
        // even when the controlled value produces no new presentation patch.
        try? applyAppearance(node, to: view)
    }

    public func controlTextDidBeginEditing(_ notification: Notification) {
        guard let field = notification.object as? NSTextField else { return }
        if let id = nodeID(for: field), let node = descriptors[id],
           let style = node["style"] as? [String: Any], let editor = field.currentEditor() as? NSTextView {
            JadixTextAppearance(style["text"]).applyEditor(editor,
                font: field.font ?? .systemFont(ofSize: 16), foreground: field.textColor ?? .textColor)
        }
        emit(field, ["kind": "focus"])
    }

    private static func scheduleFieldEditorAlignment(_ field: NSTextField) {
        DispatchQueue.main.async { [weak field] in
            guard let field, let editor = field.currentEditor() as? NSTextView else { return }
            Self.alignFieldEditor(editor, for: field)
        }
    }

    private static func alignFieldEditor(_ editor: NSTextView, for field: NSTextField) {
        guard !editor.hasMarkedText(), let window = field.window,
              window.firstResponder === editor, let clip = editor.superview as? NSClipView,
              [.left, .natural, .justified].contains(editor.alignment) else { return }
        let bounds = clip.bounds
        let rest = clip.frame.minX + editor.frame.minX + editor.textContainerOrigin.x
        let delta = bounds.minX - rest
        guard abs(delta) > 0.25 else { return }

        let selection = editor.selectedRange()
        let length = (editor.string as NSString).length
        guard selection.location <= length, selection.length <= length - selection.location else { return }
        if selection.location != 0 || selection.length != length {
            let caret = editor.firstRect(forCharacterRange: NSRange(location: NSMaxRange(selection), length: 0),
                                         actualRange: nil)
            let visible = window.convertToScreen(clip.convert(bounds, to: nil))
            if caret.height > 0,
               caret.minX >= visible.minX, caret.minX < visible.maxX,
               caret.minX + delta < visible.minX || caret.minX + delta >= visible.maxX {
                return
            }
        }
        clip.scroll(to: NSPoint(x: rest, y: bounds.minY))
    }

    public func controlTextDidEndEditing(_ notification: Notification) {
        guard let field = notification.object as? NSTextField else { return }
        emit(field, ["kind": "blur"])
    }

    public func textDidChange(_ notification: Notification) {
        guard let text = notification.object as? JadixTextView else { return }
        refreshEditorAppearance(text)
        edit(text, text: text.string)
    }

    public func textView(
        _ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString: String?
    ) -> Bool {
        allowsEdit(of: textView, in: textView, range: range, replacement: replacementString)
    }

    private func allowsEdit(of view: NSView, in editor: NSTextView, range: NSRange, replacement: String?) -> Bool {
        guard !publishing, !applyingClippedText,
              let id = nodeID(for: view), let limit = descriptors[id]?["maxLength"] as? Int,
              let replacement, !replacement.isEmpty,
              editor.undoManager?.isUndoing != true, editor.undoManager?.isRedoing != true else {
            return true
        }
        let current = editor.string as NSString
        guard range.location <= current.length, range.length <= current.length - range.location else {
            return false
        }
        let before = current.substring(to: range.location)
        let after = current.substring(from: range.location + range.length)
        let available = limit - before.unicodeScalars.count - after.unicodeScalars.count
        guard replacement.unicodeScalars.count > available else { return true }
        var used = 0
        let prefix = replacement.prefix { character in
            used += character.unicodeScalars.count
            return used <= available
        }
        if !prefix.isEmpty {
            applyingClippedText = true
            defer { applyingClippedText = false }
            editor.insertText(String(prefix), replacementRange: range)
        }
        return false
    }

    public func textDidBeginEditing(_ notification: Notification) {
        guard let text = notification.object as? JadixTextView else { return }
        emit(text, ["kind": "focus"])
    }

    public func textDidEndEditing(_ notification: Notification) {
        guard let text = notification.object as? JadixTextView else { return }
        emit(text, ["kind": "blur"])
    }
}

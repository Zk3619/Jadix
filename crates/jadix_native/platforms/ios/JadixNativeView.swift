import UIKit
import ImageIO

private final class NativeCallbackContext { weak var host: JadixNativeView?; init(_ host: JadixNativeView) { self.host = host } }

private let measureNative: JadixNativeMeasure = { context, json, output in
    precondition(Thread.isMainThread)
    guard let context = context, let json = json, let output = output else { return false }
    guard let host = Unmanaged<NativeCallbackContext>.fromOpaque(context).takeUnretainedValue().host else { return false }
    do {
        let size = try host.measure(String(cString: json))
        output[0] = Double(size.width)
        output[1] = Double(size.height)
        output[2] = host.measuredBaseline
        return true
    } catch { return false }
}

private let publishNative: JadixNativePublish = { context, json in
    precondition(Thread.isMainThread)
    guard let context = context, let json = json else { return false }
    guard let host = Unmanaged<NativeCallbackContext>.fromOpaque(context).takeUnretainedValue().host else { return false }
    do { try host.publish(String(cString: json)); return true }
    catch { return false }
}

private let clearNative: JadixNativeClear = { context in
    precondition(Thread.isMainThread)
    guard let context = context else { return }
    Unmanaged<NativeCallbackContext>.fromOpaque(context).takeUnretainedValue().host?.clear()
}

private func scheduleNative(_ context: UnsafeMutableRawPointer?, _ id: UInt64, _ json: UnsafePointer<CChar>?, _ kind: String) -> Bool {
    precondition(Thread.isMainThread)
    guard let context, let json, let host = Unmanaged<NativeCallbackContext>.fromOpaque(context).takeUnretainedValue().host else { return false }
    return host.scheduleRequest(kind, id: id, json: String(cString: json))
}
private let fileNative: JadixNativeFileRequest = { scheduleNative($0, $1, $2, "file") }
private let windowNative: JadixNativeWindowRequest = { scheduleNative($0, $1, $2, "window") }
private let alertNative: JadixNativeAlertRequest = { scheduleNative($0, $1, $2, "alert") }
private let contextNative: JadixNativeContextMenuRequest = { scheduleNative($0, $1, $2, "context") }
private let shareNative: JadixNativeShareRequest = { scheduleNative($0, $1, $2, "share") }

private enum NativeError: Error { case invalid(String) }

extension JadixNativeView {
    fileprivate func scheduleRequest(_ kind: String, id: UInt64, json: String) -> Bool {
        guard root != 0, json.utf8.count <= 1_048_576, let request = try? object(json) else { return false }
        let token = root
        if ["share", "context"].contains(kind), let owner = request["anchor_root"] as? String, owner != String(token) { return false }
        DispatchQueue.main.async { [weak self] in
            guard let self, self.root == token else { return }
            let handler: ((UInt64, [String: Any]) -> Bool)?
            switch kind { case "file": handler = self.onFileRequest; case "alert": handler = self.onAlertRequest; case "share": handler = self.onShareRequest; case "context": handler = self.onContextMenuRequest; default: handler = self.onWindowRequest }
            if handler?(id, request) == true { return }
            self.systemServices.handle(kind, id: id, request: request)
        }
        return true
    }
    /// Complete an overridden service request on the main actor. Completion is
    /// deferred so synchronous presenters cannot re-enter a borrowed Rust root.
    public func completeRequest(_ kind: String, id: UInt64, result: [String: Any]) {
        let token = root
        guard ["file", "alert", "share", "context", "window"].contains(kind), token != 0, let data = try? JSONSerialization.data(withJSONObject: result), data.count <= 1_048_576 else { return }
        let json = String(decoding: data, as: UTF8.self)
        DispatchQueue.main.async { [weak self] in
            guard self?.root == token else { return }
            json.withCString {
                switch kind { case "file": _ = jadix_native_file_result(token, id, $0); case "alert": _ = jadix_native_alert_result(token, id, $0); case "share": _ = jadix_native_share_result(token, id, $0); case "context": _ = jadix_native_context_menu_result(token, id, $0); default: _ = jadix_native_window_result(token, id, $0) }
            }
        }
    }
}

private final class NativeCheckbox: UIControl {
    let toggle = UISwitch()
    let caption = UILabel()
    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(toggle); addSubview(caption)
        caption.isAccessibilityElement = false
        toggle.addTarget(self, action: #selector(changed), for: .valueChanged)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    @objc private func changed() { sendActions(for: .valueChanged) }
    override func sizeThatFits(_ size: CGSize) -> CGSize {
        let control = toggle.sizeThatFits(size)
        let text = caption.sizeThatFits(CGSize(width: max(0, size.width - control.width - 8), height: size.height))
        return CGSize(width: min(size.width, control.width + text.width + 8), height: min(size.height, max(control.height, text.height)))
    }
    override func layoutSubviews() {
        super.layoutSubviews()
        let size = toggle.sizeThatFits(bounds.size)
        toggle.frame = CGRect(x: effectiveUserInterfaceLayoutDirection == .rightToLeft ? bounds.width - size.width : 0, y: (bounds.height - size.height) / 2, width: size.width, height: size.height)
        caption.frame = CGRect(x: effectiveUserInterfaceLayoutDirection == .rightToLeft ? 0 : size.width + 8, y: 0, width: max(0, bounds.width - size.width - 8), height: bounds.height)
    }
}

private final class NativeTextView: UITextView {
    let placeholder = UILabel()
    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        addSubview(placeholder)
        placeholder.textColor = .placeholderText
        placeholder.isAccessibilityElement = false
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    override func layoutSubviews() {
        super.layoutSubviews()
        placeholder.frame = CGRect(x: textContainerInset.left + textContainer.lineFragmentPadding, y: textContainerInset.top,
            width: max(0, bounds.width - textContainerInset.left - textContainerInset.right - textContainer.lineFragmentPadding * 2), height: font?.lineHeight ?? 20)
        placeholder.isHidden = !text.isEmpty
    }
}

private final class NativeModalController: UIViewController {
    var moveFocus: ((Bool) -> Bool)?
    let content: UIView
    let dismissDialog: (Bool) -> Void
    private let backdrop = UIButton(type: .custom)
    init(content: UIView, dismiss: @escaping (Bool) -> Void) {
        self.content = content; self.dismissDialog = dismiss
        super.init(nibName: nil, bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .clear
        view.accessibilityViewIsModal = true
        backdrop.backgroundColor = UIColor.black.withAlphaComponent(0.32)
        backdrop.isAccessibilityElement = false
        backdrop.addTarget(self, action: #selector(outside), for: .touchUpInside)
        view.addSubview(backdrop); view.addSubview(content)
        content.accessibilityViewIsModal = true
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        backdrop.frame = view.bounds
    }
    override var canBecomeFirstResponder: Bool { true }
    override var keyCommands: [UIKeyCommand]? {
        [UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(cancelDialog)),
         UIKeyCommand(input: "\t", modifierFlags: [], action: #selector(tab)),
         UIKeyCommand(input: "\t", modifierFlags: [.shift], action: #selector(tab))]
    }
    @objc private func tab(_ command: UIKeyCommand) { _ = moveFocus?(command.modifierFlags.contains(.shift)) }
    override func accessibilityPerformEscape() -> Bool { dismissDialog(false); return true }
    @objc private func outside() { dismissDialog(true) }
    @objc private func cancelDialog() { dismissDialog(false) }
}

final class NativeSelect: JadixButton {
    private var signature: NSDictionary?
    private var generation: UInt64 = 0
    var onSelect: ((String) -> Void)?

    func requestSelection(_ value: String) { onSelect?(value) }

    func configure(_ node: [String: Any]) throws {
        guard let options = node["options"] as? [[String: Any]], let value = node["value"] as? String,
              let placeholder = node["placeholder"] as? String, let enabled = node["enabled"] as? Bool else {
            throw NativeError.invalid("invalid native Select descriptor")
        }
        let selected = options.first { $0["value"] as? String == value }
        guard value.isEmpty || selected != nil else { throw NativeError.invalid("unknown native Select value") }
        let label = selected?["label"] as? String ?? placeholder
        setTitle(label, for: .normal)
        accessibilityLabel = node["name"] as? String ?? placeholder
        accessibilityHint = node["description"] as? String
        accessibilityValue = value.isEmpty ? nil : label
        isEnabled = enabled
        showsMenuAsPrimaryAction = true
        let next = NSDictionary(dictionary: ["options": options, "value": value, "placeholder": placeholder, "enabled": enabled])
        if signature?.isEqual(next) == true { return }
        let (nextGeneration, overflow) = generation.addingReportingOverflow(1)
        guard !overflow else { throw NativeError.invalid("native Select menu generation exhausted") }
        contextMenuInteraction?.dismissMenu()
        generation = nextGeneration
        var actions: [UIAction] = []
        for option in options {
            guard let optionValue = option["value"] as? String, let label = option["label"] as? String,
                  let optionEnabled = option["enabled"] as? Bool else { throw NativeError.invalid("invalid native Select option") }
            let action = UIAction(title: label, identifier: UIAction.Identifier(rawValue: optionValue),
                                  attributes: optionEnabled ? [] : [.disabled], state: optionValue == value ? .on : .off) { [weak self] _ in
                guard let self = self, self.generation == nextGeneration else { return }
                self.requestSelection(optionValue)
            }
            actions.append(action)
        }
        menu = UIMenu(title: placeholder, children: actions)
        signature = next
    }

    func retire() {
        contextMenuInteraction?.dismissMenu()
        menu = nil
        onSelect = nil
        signature = nil
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil { contextMenuInteraction?.dismissMenu() }
    }
}

private final class NativeModal: UIView {
    var moveFocus: ((Bool) -> Bool)?
    let content = UIView()
    var modalWindow: UIWindow?
    weak var previousWindow: UIWindow?
    weak var previousFocus: UIView?
    var dismissDialog: ((Bool) -> Void)?

    private func focused(_ view: UIView) -> UIView? {
        if view.isFirstResponder { return view }
        for child in view.subviews { if let focus = focused(child) { return focus } }
        return nil
    }

    func show(in host: UIView, frame: CGRect, name: String, description: String?) throws {
        if modalWindow == nil {
            guard let source = host.window else { throw NativeError.invalid("dialog requires an attached native window") }
            let window: UIWindow
            if let scene = source.windowScene {
                window = UIWindow(windowScene: scene)
                previousWindow = scene.windows.first(where: { $0.isKeyWindow }) ?? source
            } else {
                window = UIWindow(frame: source.frame)
                previousWindow = UIApplication.shared.windows.first(where: { $0.isKeyWindow }) ?? source
            }
            previousFocus = previousWindow.flatMap { focused($0) }
            window.windowLevel = .alert + 1
            let controller = NativeModalController(content: content) { [weak self] backdrop in self?.dismissDialog?(backdrop) }
            controller.moveFocus = { [weak self] in self?.moveFocus?($0) == true }
            window.rootViewController = controller
            modalWindow = window
            window.makeKeyAndVisible()
            UIAccessibility.post(notification: .screenChanged, argument: content)
        }
        guard let window = modalWindow else { return }
        window.rootViewController?.loadViewIfNeeded()
        let positionedFrame = host.convert(frame, to: window)
        content.bounds = CGRect(origin: .zero, size: positionedFrame.size)
        content.center = CGPoint(x: positionedFrame.midX, y: positionedFrame.midY)
        content.accessibilityLabel = name
        content.accessibilityHint = description
    }

    func hide() {
        guard let window = modalWindow else { return }
        window.endEditing(true)
        window.isHidden = true
        modalWindow = nil
        previousWindow?.makeKey()
        if previousFocus?.window != nil { _ = previousFocus?.becomeFirstResponder() }
        UIAccessibility.post(notification: .screenChanged, argument: previousFocus ?? previousWindow)
        previousFocus = nil; previousWindow = nil
    }
}

public final class JadixNativeView: UIView, UITextFieldDelegate, UITextViewDelegate, UIScrollViewDelegate {
    public typealias RootFactory = (UnsafeMutableRawPointer?, JadixNativeMeasure?, JadixNativePublish?, JadixNativeClear?, JadixNativeFileRequest?, JadixNativeWindowRequest?, JadixNativeAlertRequest?, JadixNativeShareRequest?, Float, Float) -> UInt64
    private lazy var callbackContext = NativeCallbackContext(self)
    private var root: UInt64 = 0
    public var rootID: UInt64 { root }
    private lazy var systemServices = JadixSystemServices(host: self)
    fileprivate var measuredBaseline = Double.nan
    private var pollTimer: Timer?
    private var fetchService = JadixFetchService(), audioService = JadixAudioService(), speechService = JadixSpeechService(), desktopServices = JadixDesktopServices()
    private var environmentObservers: [NSObjectProtocol] = []
    private var configuredRemoteImagePolicy = JadixRemoteImagePolicy()
    private var remoteImages: AppleRemoteImages?
    private var imageLayoutPending = false
    public func restrictRemoteImages(to policy: JadixRemoteImagePolicy) throws {
        guard root == 0 else { throw NativeError.invalid("image policy must be configured before mount") }; configuredRemoteImagePolicy = policy
    }
    private func imageLoader() -> AppleRemoteImages {
        if let remoteImages { return remoteImages }; let loader = AppleRemoteImages(policy: configuredRemoteImagePolicy); remoteImages = loader; return loader
    }
    private var viewAppearances: [String: JadixViewAppearance] = [:]
    private var embeddedFactories: [String: JadixEmbeddedViewFactory] = [:]
    private var webResourceReplies: [UInt64: (Result<JadixWebResourceResponse, Error>) -> Void] = [:]
    private let handoff = JadixHandoff()
    public var handoffEnabled = false { didSet { refreshHandoff() } }
    var handoffActive = true { didSet { refreshHandoff() } }
    public override var isHidden: Bool { didSet { refreshHandoff() } }
    public static var declaresHandoff: Bool { JadixHandoff.declared }
    public var notificationScope = "main"
    public var onWindowRequest: ((UInt64, [String: Any]) -> Bool)?
    public var onFileRequest: ((UInt64, [String: Any]) -> Bool)?
    public var onAlertRequest: ((UInt64, [String: Any]) -> Bool)?
    public var onContextMenuRequest: ((UInt64, [String: Any]) -> Bool)?
    public var onShareRequest: ((UInt64, [String: Any]) -> Bool)?
    public func registerEmbeddedView(_ kind: String, factory: @escaping JadixEmbeddedViewFactory) throws {
        guard root == 0, embeddedFactories.count < 64, embeddedFactories[kind] == nil, !kind.isEmpty,
              kind.utf8.count <= 128, kind.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || [45,46,95].contains($0) }) else { throw NativeError.invalid("invalid embedded view registration") }
        embeddedFactories[kind] = factory
    }
    private var environment: UInt64 = 1
    private var publishing = false
    private var viewportSnapshot: [String: Any]?
    private var cullingGeneration: UInt64 = 0
    private var interactions: [String: JadixInteractionInput] = [:]
    private var views: [String: UIView] = [:]
    private var descriptors: [String: [String: Any]] = [:]
    private var preboundViews: [String: (key: String, view: UIView)] = [:]
    private var recycledViews: [String: [UIView]] = [:]
    private var recycledViewCount = 0
    private var identities: [ObjectIdentifier: String] = [:]
    private var nodeOrder: [String] = []
    private var accessibilityActiveDescendants: [String: String] = [:]
    private var revisions: [String: UInt64] = [:]
    private var modalOrder: [String] = []
    private var popoverOrder: [String] = []
    private var popoverUpdatePending = false
    private var lastSize = CGSize.zero
    private var appliedContentSizeCategory: UIContentSizeCategory?
    public var onError: ((Error) -> Void)?
    public var onStateChange: ((String) -> Void)?
    public var onPopoverEvent: ((String, JadixPopoverEvent) -> Void)?

    public func reportWindowState(_ state: [String: Any]) {
        guard root != 0, let data = try? JSONSerialization.data(withJSONObject: state) else { return }
        String(decoding: data, as: UTF8.self).withCString { _ = jadix_native_window_state(root, $0) }
    }
    public func reportWindowEvent(_ event: [String: Any]) {
        guard root != 0, let data = try? JSONSerialization.data(withJSONObject: event) else { return }
        String(decoding: data, as: UTF8.self).withCString { _ = jadix_native_window_event(root, $0) }
    }
    public func configureDeepLinks(schemes: [String], singleInstance: Bool = false, queueLimit: Int = 64) throws {
        guard root != 0 else { throw NativeError.invalid("unmounted root") }
        let data = try JSONSerialization.data(withJSONObject: ["schemes": schemes, "single_instance": singleInstance, "queue_limit": queueLimit])
        _ = try String(decoding: data, as: UTF8.self).withCString { try reply(jadix_native_deep_link(root, $0, true)) }
    }
    public func deliverDeepLink(_ url: URL, source: String = "hot_start") throws {
        guard root != 0, url.absoluteString.utf8.count <= 16384 else { throw NativeError.invalid("invalid deep link or unmounted root") }
        let data = try JSONSerialization.data(withJSONObject: ["url": url.absoluteString, "source": source])
        _ = try String(decoding: data, as: UTF8.self).withCString { try reply(jadix_native_deep_link(root, $0, false)) }
    }

    public func setAppAddress(_ address: String?) throws {
        guard root != 0 else { throw NativeError.invalid("unmounted native root") }
        if let address {
            guard !address.utf8.contains(0) else { throw NativeError.invalid("invalid app address") }
            _ = try address.withCString { try reply(jadix_native_set_app_address(root, $0)) }
        } else { _ = try reply(jadix_native_set_app_address(root, nil)) }
    }
    @discardableResult public func continueUserActivity(_ activity: NSUserActivity) -> Bool {
        guard handoffEnabled, let url = JadixHandoff.continuedURL(activity) else { return false }
        do { try deliverDeepLink(url, source: "handoff"); return true }
        catch { onError?(error); return false }
    }
    private func refreshHandoff() {
        handoff.refresh(enabled: handoffEnabled, active: root != 0 && handoffActive && !isHidden
            && window?.isKeyWindow == true && window?.windowScene?.activationState == .foregroundActive)
    }

    public func mount(factory: RootFactory? = nil) throws {
        precondition(Thread.isMainThread)
        guard root == 0 else { throw NativeError.invalid("root already mounted") }
        let create: RootFactory = factory ?? { jadix_native_create_with_services($0,$1,$2,$3,$4,$5,$6,$7,$8,$9) }
        root = create(Unmanaged.passUnretained(callbackContext).toOpaque(), measureNative, publishNative,
                      clearNative, fileNative, windowNative, alertNative, shareNative, Float(bounds.width), Float(bounds.height))
        guard root != 0 else { throw NativeError.invalid("native mount failed") }
        guard jadix_native_enable_context_menus(root, contextNative) else { close(); throw NativeError.invalid("context menu bridge failed") }
        environmentObservers = [NSLocale.currentLocaleDidChangeNotification, .NSSystemTimeZoneDidChange,
            UIContentSizeCategory.didChangeNotification, UIAccessibility.reduceMotionStatusDidChangeNotification,
            UIAccessibility.darkerSystemColorsStatusDidChangeNotification, UIAccessibility.reduceTransparencyStatusDidChangeNotification,
            UIAccessibility.voiceOverStatusDidChangeNotification, UIApplication.didBecomeActiveNotification,
            UIApplication.willResignActiveNotification, UIApplication.didEnterBackgroundNotification,
            UIApplication.willEnterForegroundNotification, UIWindow.didBecomeKeyNotification, UIWindow.didResignKeyNotification,
            UIScene.didActivateNotification, UIScene.willDeactivateNotification, .jadixFontsChanged].map { name in
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                DispatchQueue.main.async { [weak self] in
                    guard let self else { return }
                    if [UIApplication.willResignActiveNotification, UIApplication.didEnterBackgroundNotification,
                        UIWindow.didResignKeyNotification, UIScene.willDeactivateNotification].contains(name) {
                        for input in Array(self.interactions.values) { input.cancel() }
                        for view in self.views.values {
                            (view as? JadixContainer)?.keyboardInput.cancel()
                            (view as? JadixButton)?.keyboardInput.cancel()
                        }
                    }
                    self.invalidateEnvironment(); self.refreshHandoff(); self.scheduleCulling()
                }
            }
        }
        lastSize = bounds.size
        appliedContentSizeCategory = traitCollection.preferredContentSizeCategory
        desktopServices.scope = notificationScope
        let mountedRoot = root
        systemServices.alive = { [weak self] in self?.root == mountedRoot }
        systemServices.lookup = { [weak self] in self?.views[$0] }
        systemServices.respond = { [weak self] kind, id, result in self?.completeRequest(kind, id: id, result: result) }
        desktopServices.authAnchor = { [weak self] in self?.window }
        desktopServices.output = { [weak self] id, event in
            DispatchQueue.main.async { [weak self] in
                guard self?.root == mountedRoot, let data = try? JSONSerialization.data(withJSONObject: event), data.count <= 128 * 1024 else { return }
                String(decoding: data, as: UTF8.self).withCString { _ = jadix_native_desktop_service_event(mountedRoot,id,$0) }
            }
        }
        desktopServices.start()
        let timer = Timer(timeInterval: 1 / 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { guard let self, self.root == mountedRoot else { return }; jadix_native_web_poll(mountedRoot) }
        }
        pollTimer = timer; RunLoop.main.add(timer, forMode: .common)
        invalidateEnvironment(); schedulePopovers(); refreshHandoff()
    }

    public func close() {
        precondition(Thread.isMainThread)
        onStateChange = nil
        let previous = root
        root = 0
        systemServices.dispose()
        environmentObservers.forEach(NotificationCenter.default.removeObserver); environmentObservers.removeAll()
        remoteImages?.invalidate(); remoteImages = nil
        pollTimer?.invalidate(); pollTimer = nil
        fetchService.dispose(); audioService.dispose(); speechService.dispose(); desktopServices.dispose()
        fetchService = JadixFetchService(); audioService = JadixAudioService(); speechService = JadixSpeechService(); desktopServices = JadixDesktopServices()
        let replies = webResourceReplies; webResourceReplies.removeAll()
        for reply in replies.values { reply(.failure(URLError(.cancelled))) }
        if previous != 0 { jadix_native_dispose(previous) }
        clear()
    }

    public func state() throws -> String {
        precondition(Thread.isMainThread)
        guard root != 0 else { throw NativeError.invalid("unmounted native root") }
        return try reply(jadix_native_state(root))
    }

    public func restore(_ json: String) throws {
        precondition(Thread.isMainThread)
        guard root != 0 else { throw NativeError.invalid("unmounted native root") }
        _ = try json.withCString { try reply(jadix_native_restore(root, $0)) }
    }

    public func storageResult(_ error: String?) throws {
        precondition(Thread.isMainThread)
        guard root != 0 else { throw NativeError.invalid("unmounted native root") }
        if let error = error {
            _ = try error.withCString { try reply(jadix_native_storage_result(root, $0)) }
        } else {
            _ = try reply(jadix_native_storage_result(root, nil))
        }
    }

    deinit { pollTimer?.invalidate(); environmentObservers.forEach(NotificationCenter.default.removeObserver); if root != 0 { jadix_native_dispose(root) } }

    public override func layoutSubviews() {
        super.layoutSubviews()
        if root != 0 && bounds.size != lastSize {
            lastSize = bounds.size
            invalidateEnvironment()
            revealFocusedControl()
        }
        schedulePopovers()
    }

    public override func didMoveToWindow() {
        super.didMoveToWindow()
        refreshHandoff()
        if window == nil {
            for id in popoverOrder.reversed() { (views[id] as? NativePopover)?.hide(restore: false) }
        } else { schedulePopovers() }
    }

    public override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let names: [UIKeyboardHIDUsage: String] = [.keyboardUpArrow: "up", .keyboardDownArrow: "down", .keyboardLeftArrow: "left", .keyboardRightArrow: "right",
            .keyboardHome: "home", .keyboardEnd: "end", .keyboardPageUp: "pageUp", .keyboardPageDown: "pageDown", .keyboardTab: "tab", .keyboardEscape: "escape", .keyboardReturnOrEnter: "enter"]
        var remaining = presses
        if !hasMarkedText, let focused = focusedDescendant {
            var candidate: UIView? = focused
            while let view = candidate, identities[ObjectIdentifier(view)] == nil { candidate = view.superview }
            if let view = candidate, let id = identities[ObjectIdentifier(view)], let node = descriptors[id], node["enabled"] as? Bool != false {
            for press in presses {
                guard let key = press.key, key.modifierFlags.intersection([.shift, .control, .command, .alternate]).isEmpty,
                      let name = names[key.keyCode], (node["navigationKeys"] as? [String])?.contains(name) == true else { continue }
                emit(view, ["kind": "navigate", "key": name, "shift": key.modifierFlags.contains(.shift), "control": key.modifierFlags.contains(.control)])
                remaining.remove(press)
            }
            }
        }
        if !hasMarkedText {
            for press in remaining {
                guard let key = press.key, key.keyCode == .keyboardTab,
                      key.modifierFlags.intersection([.control, .command, .alternate]).isEmpty,
                      moveTabFocus(backwards: key.modifierFlags.contains(.shift)) else { continue }
                remaining.remove(press)
            }
        }
        if !remaining.isEmpty { super.pressesBegan(remaining, with: event) }
    }

    private var focusedDescendant: UIView? {
        func find(_ view: UIView) -> UIView? {
            if view.isFirstResponder { return view }
            for child in view.subviews { if let focused = find(child) { return focused } }
            return nil
        }
        if let focus = find(self) { return focus }
        for id in (modalOrder + popoverOrder).reversed() {
            if let modal = views[id] as? NativeModal, let root = modal.modalWindow?.rootViewController?.view, let focus = find(root) { return focus }
            if let popover = views[id] as? NativePopover, let focus = find(popover.content) { return focus }
        }
        return nil
    }
    private func nodeID(for view: UIView?) -> String? {
        var current = view
        while let candidate = current {
            if let id = identities[ObjectIdentifier(candidate)] { return id }
            current = candidate.superview
        }
        return nil
    }
    @discardableResult private func moveTabFocus(backwards: Bool) -> Bool {
        let scope = popoverOrder.last ?? modalOrder.last
        func inScope(_ id: String) -> Bool {
            guard let scope else { return true }
            var current: String? = id
            while let key = current { if key == scope { return true }; current = descriptors[key]?["parent"] as? String }
            return false
        }
        let candidates = nodeOrder.filter { id in
            guard let node = descriptors[id], let view = views[id], eventVisible(id), inScope(id),
                  view.window != nil, node["enabled"] as? Bool != false, node["tabStop"] as? Bool != false else { return false }
            return ["button", "input", "textarea", "checkbox", "switch", "select", "hosted"].contains(node["kind"] as? String ?? "") || node["tabStop"] as? Bool == true
        }
        guard !candidates.isEmpty else { return scope != nil }
        let current = nodeID(for: focusedDescendant).flatMap { candidates.firstIndex(of: $0) }
        let start = current ?? (backwards ? 0 : -1), step = backwards ? -1 : 1
        for offset in 1...candidates.count {
            let id = candidates[(start + step * offset + candidates.count * 2) % candidates.count]
            guard let view = views[id], jadixFocusView(view) else { continue }
            emit(view, ["kind": "focusVisible"]); revealFocusedControl(); return true
        }
        return scope != nil
    }
    private func scheduleFocus(_ id: String?) {
        let token = root
        DispatchQueue.main.async { [weak self] in
            guard let self, self.root == token, token != 0 else { return }
            if let id, self.eventVisible(id), let view = self.views[id], self.descriptors[id]?["enabled"] as? Bool != false {
                if jadixFocusView(view) { self.revealFocusedControl() }
            } else if id == nil { self.focusedDescendant?.resignFirstResponder() }
        }
    }
    var hasMarkedText: Bool { (focusedDescendant as? UITextInput)?.markedTextRange != nil }
    private func revealFocusedControl() {
        guard !publishing, root != 0, let focused = focusedDescendant else { return }
        var parent = focused.superview
        while let view = parent, view !== self {
            if let id = identities[ObjectIdentifier(view)], descriptors[id]?["kind"] as? String == "scroll",
               let scroll = view as? UIScrollView {
                scroll.scrollRectToVisible(focused.convert(focused.bounds, to: scroll), animated: false)
            }
            parent = view.superview
        }
    }

    public override func traitCollectionDidChange(_ previousTraitCollection: UITraitCollection?) {
        super.traitCollectionDidChange(previousTraitCollection)
        if previousTraitCollection != traitCollection {
            invalidateEnvironment()
        }
    }

    public func invalidateEnvironment() {
        precondition(Thread.isMainThread)
        guard root != 0 else { return }
        do {
            let category = traitCollection.preferredContentSizeCategory
            if appliedContentSizeCategory != category || !descriptors.isEmpty {
                let previousPublishing = publishing
                publishing = true
                defer { publishing = previousPublishing }
                for (id, view) in views {
                    if let node = descriptors[id] { try applyTypography(node, to: view) }
                }
                appliedContentSizeCategory = category
            }
            let (nextEnvironment, overflow) = environment.addingReportingOverflow(1)
            guard !overflow else { return }; environment = nextEnvironment
            let data = try JSONSerialization.data(withJSONObject: jadixEnvironmentSnapshot(self))
            _ = try String(decoding: data, as: UTF8.self).withCString { try reply(jadix_native_environment(root, $0)) }
            _ = try reply(jadix_native_resize(root, Float(bounds.width), Float(bounds.height), environment))
        }
        catch { onError?(error) }
    }

    private func reply(_ pointer: UnsafeMutablePointer<CChar>?) throws -> String {
        guard let pointer = pointer else { throw NativeError.invalid("missing native reply") }
        defer { jadix_native_string_free(pointer) }
        let reply = try object(String(cString: pointer))
        guard reply["ok"] as? Bool == true else { throw NativeError.invalid(reply["error"] as? String ?? "native failure") }
        return reply["value"] as? String ?? ""
    }

    private func object(_ text: String) throws -> [String: Any] {
        guard let data = text.data(using: .utf8), let value = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw NativeError.invalid("invalid native JSON")
        }
        return value
    }

    private func make(_ node: [String: Any]) throws -> UIView {
        switch node["kind"] as? String {
        case "hosted": return JadixHostedView()
        case "box": return JadixContainer()
        case "dialog": return NativeModal()
        case "popover": return NativePopover(frame: .zero)
        case "scroll": let view = JadixScrollView(); view.delegate = self; return view
        case "text": let view = JadixLabel(); view.numberOfLines = 0; return view
        case "image":
            let view = JadixImageView(remoteImages: imageLoader()); view.isAccessibilityElement = true; return view
        case "button":
            let view = JadixButton(type: .system)
            view.addTarget(self, action: #selector(activate(_:)), for: .touchUpInside)
            return view
        case "select": return NativeSelect(type: .system)
        case "input":
            let view = UITextField()
            view.borderStyle = .roundedRect
            view.returnKeyType = .done
            view.delegate = self
#if JADIX_NATIVE_CONSISTENT_UI
            view.inputAccessoryView = keyboardAccessoryView()
#endif
            view.addTarget(self, action: #selector(edited(_:)), for: .editingChanged)
            view.addTarget(self, action: #selector(focused(_:)), for: .editingDidBegin)
            return view
        case "textarea":
            let view = NativeTextView(frame: .zero, textContainer: nil)
            view.delegate = self
#if JADIX_NATIVE_CONSISTENT_UI
            view.inputAccessoryView = keyboardAccessoryView()
#endif
            return view
        case "checkbox", "switch":
            let view = NativeCheckbox(frame: .zero)
            view.addTarget(self, action: #selector(toggled(_:)), for: .valueChanged)
            return view
        default: throw NativeError.invalid("unsupported UIKit node")
        }
    }

#if JADIX_NATIVE_CONSISTENT_UI
    private func keyboardAccessoryView() -> UIToolbar {
        let toolbar = UIToolbar()
        toolbar.sizeToFit()
        toolbar.items = [
            UIBarButtonItem(systemItem: .flexibleSpace),
            UIBarButtonItem(barButtonSystemItem: .done, target: self, action: #selector(dismissKeyboard))
        ]
        return toolbar
    }

    @objc private func dismissKeyboard() { endEditing(true) }
#endif

    private func color(_ value: Any?) -> UIColor {
        guard let channels = value as? [Double], channels.count == 4 else { return .clear }
        return UIColor(red: CGFloat(channels[0] / 255), green: CGFloat(channels[1] / 255),
                       blue: CGFloat(channels[2] / 255), alpha: CGFloat(channels[3] / 255))
    }

    private func applyTypography(_ node: [String: Any], to view: UIView) throws {
        guard let style = node["style"] as? [String: Any], let size = style["font_size"] as? Double,
              let weight = style["font_weight"] as? Double else { throw NativeError.invalid("invalid native style") }
        let selectedFont = JadixTextAppearance.resolvedFont(families: style["font_family"] as? String ?? "system-ui", size: CGFloat(size), weight: weight)
        let appearance = JadixTextAppearance(style["text"])
        let font = appearance.font(UIFontMetrics.default.scaledFont(for: selectedFont, compatibleWith: traitCollection))
        view.semanticContentAttribute = appearance.direction == .rightToLeft ? .forceRightToLeft : appearance.direction == .leftToRight ? .forceLeftToRight : .unspecified
        if let label = view as? UILabel {
            label.font = font
            label.adjustsFontForContentSizeCategory = false
        }
        if let button = view as? UIButton {
            button.titleLabel?.font = font
            button.titleLabel?.adjustsFontForContentSizeCategory = false
        }
        if let input = view as? UITextField {
            input.font = font
            input.adjustsFontForContentSizeCategory = false
        }
        if let input = view as? NativeTextView {
            input.font = font
            input.adjustsFontForContentSizeCategory = false
            input.placeholder.font = font
            input.placeholder.adjustsFontForContentSizeCategory = false
            input.setNeedsLayout()
        }
        if let checkbox = view as? NativeCheckbox {
            checkbox.caption.font = font
            checkbox.caption.adjustsFontForContentSizeCategory = false
        }
    }

    private func apply(_ node: [String: Any], to view: UIView, preserveComposition: Bool = false) throws {
        try applyTypography(node, to: view)
        guard let style = node["style"] as? [String: Any] else { throw NativeError.invalid("invalid native style") }
        view.backgroundColor = color(style["background"])
        view.accessibilityLabel = node["name"] as? String
        view.accessibilityHint = node["description"] as? String
        view.accessibilityTraits.remove([.header, .selected])
        view.shouldGroupAccessibilityChildren = false
        if view is JadixContainer { view.isAccessibilityElement = false; view.accessibilityValue = nil }
        if let image = view as? JadixImageView {
            image.setSource(node["source"] as? String, style: style)
            image.accessibilityLabel = node["name"] as? String ?? node["alt"] as? String
        }
        let textAppearance = JadixTextAppearance(style["text"])
        if let label = view as? UILabel {
            let overflow = style["text_overflow"] as? String ?? "wrap"
            label.numberOfLines = (style["max_lines"] as? Int) ?? (overflow == "wrap" ? 0 : 1)
            switch overflow {
            case "wrap": label.lineBreakMode = .byWordWrapping
            case "clip": label.lineBreakMode = .byClipping
            case "ellipsis_start": label.lineBreakMode = .byTruncatingHead
            case "ellipsis_middle": label.lineBreakMode = .byTruncatingMiddle
            default: label.lineBreakMode = .byTruncatingTail
            }
            let attributes = textAppearance.attributes(font: label.font, foreground: color(style["foreground"]), lineBreak: label.lineBreakMode)
            let string = NSMutableAttributedString(string: node["text"] as? String ?? "", attributes: attributes)
            var boundaries = [0: 0], byte = 0, utf16 = 0
            for character in string.string {
                let value = String(character); byte += value.utf8.count; utf16 += value.utf16.count; boundaries[byte] = utf16
            }
            for entry in style["inline_styles"] as? [[String: Any]] ?? [] {
                guard let range = entry["range"] as? [Int], range.count == 2,
                      let start = boundaries[range[0]], let end = boundaries[range[1]], end >= start else { continue }
                var attributes: [NSAttributedString.Key: Any] = [:]
                if let foreground = JadixVisualStyle.color(entry["foreground"]) { attributes[.foregroundColor] = foreground }
                if let background = JadixVisualStyle.color(entry["background"]) { attributes[.backgroundColor] = background }
                var traits = label.font.fontDescriptor.symbolicTraits
                if let bold = entry["bold"] as? Bool { if bold { traits.insert(.traitBold) } else { traits.remove(.traitBold) } }
                if let italic = entry["italic"] as? Bool { if italic { traits.insert(.traitItalic) } else { traits.remove(.traitItalic) } }
                if let descriptor = label.font.fontDescriptor.withSymbolicTraits(traits) { attributes[.font] = UIFont(descriptor: descriptor, size: label.font.pointSize) }
                if let underline = entry["underline"] as? Bool { attributes[.underlineStyle] = underline ? NSUnderlineStyle.single.rawValue : 0 }
                if let strike = entry["strikethrough"] as? Bool { attributes[.strikethroughStyle] = strike ? NSUnderlineStyle.single.rawValue : 0 }
                string.addAttributes(attributes, range: NSRange(location: start, length: end - start))
            }
            label.attributedText = textAppearance.display(string)
            label.accessibilityLabel = node["name"] as? String ?? node["text"] as? String
        }
        if let button = view as? UIButton {
            button.setTitle(node["text"] as? String, for: .normal)
            if let font = button.titleLabel?.font { button.setAttributedTitle(textAppearance.display(NSAttributedString(string: node["text"] as? String ?? "", attributes: textAppearance.attributes(font: font, foreground: color(style["foreground"])))), for: .normal) }
            button.isEnabled = node["enabled"] as? Bool ?? false
            button.setTitleColor(color(style["foreground"]), for: .normal)
            if node["role"] as? String == "radio" {
                button.isSelected = node["selected"] as? Bool ?? false
                button.accessibilityTraits = button.isSelected ? [.button, .selected] : [.button]
                button.accessibilityValue = button.isSelected ? "Selected" : "Not selected"
            } else {
                button.isSelected = false
                button.accessibilityTraits = .button
                button.accessibilityValue = nil
            }
        }
        if let role = node["role"] as? String {
            if role == "heading" || role == "columnheader" { view.accessibilityTraits.insert(.header) }
            if ["tab", "radio", "option", "treeitem"].contains(role) {
                view.isAccessibilityElement = true
                if node["selected"] as? Bool == true { view.accessibilityTraits.insert(.selected) }
                else { view.accessibilityTraits.remove(.selected) }
                if let expanded = node["expanded"] as? Bool { view.accessibilityValue = expanded ? "Expanded" : "Collapsed" }
            }
            if ["tablist", "radiogroup", "listbox", "tree"].contains(role) { view.shouldGroupAccessibilityChildren = true }
        }
        if let select = view as? NativeSelect { try select.configure(node) }
        if let input = view as? UITextField {
            let value = node["text"] as? String ?? ""
            let password = node["password"] as? Bool ?? false
            if input.isSecureTextEntry != password { input.isSecureTextEntry = password }
            if input.text != value && !(preserveComposition && input.markedTextRange != nil) {
                let selection = input.selectedTextRange.map { NSRange(location: input.offset(from: input.beginningOfDocument, to: $0.start), length: input.offset(from: $0.start, to: $0.end)) }
                input.unmarkText(); input.undoManager?.removeAllActions(); input.text = value
                if let selection {
                    let clamped = JadixRichText.selection(selection, in: value)
                    if let start = input.position(from: input.beginningOfDocument, offset: clamped.location), let end = input.position(from: start, offset: clamped.length) { input.selectedTextRange = input.textRange(from: start, to: end) }
                }
            }
            if input.markedTextRange == nil, let font = input.font {
                let selection = input.selectedTextRange.map { [input.offset(from: input.beginningOfDocument, to: $0.start), input.offset(from: input.beginningOfDocument, to: $0.end)] }
                let attributes = textAppearance.attributes(font: font, foreground: color(style["foreground"]))
                input.defaultTextAttributes = attributes
                if let selection, let start = input.position(from: input.beginningOfDocument, offset: selection[0]), let end = input.position(from: input.beginningOfDocument, offset: selection[1]) { input.selectedTextRange = input.textRange(from: start, to: end) }
                input.tintColor = JadixVisualStyle.color(textAppearance.values["caret_color"]) ?? .systemBlue
            }
            input.attributedPlaceholder = NSAttributedString(string: node["placeholder"] as? String ?? "", attributes: [.font: input.font ?? UIFont.systemFont(ofSize: 14), .foregroundColor: JadixVisualStyle.color(textAppearance.values["placeholder_color"]) ?? UIColor.placeholderText])
            input.isEnabled = node["enabled"] as? Bool ?? false
            input.textColor = color(style["foreground"])
        }
        if let input = view as? NativeTextView {
            let value = node["text"] as? String ?? ""
            if input.text != value && !(preserveComposition && input.markedTextRange != nil) {
                let selection = input.selectedRange
                input.unmarkText(); input.undoManager?.removeAllActions(); input.text = value
                input.selectedRange = JadixRichText.selection(selection, in: value)
            }
            input.placeholder.text = node["placeholder"] as? String
            input.isEditable = node["enabled"] as? Bool ?? false
            input.isSelectable = true
            input.textColor = color(style["foreground"])
            if let font = input.font { textAppearance.applyEditor(input, font: font, foreground: color(style["foreground"])) }
            input.placeholder.textColor = JadixVisualStyle.color(textAppearance.values["placeholder_color"]) ?? .placeholderText
            input.setNeedsLayout()
        }
        if let checkbox = view as? NativeCheckbox {
            checkbox.toggle.isOn = node["checked"] as? Bool ?? false
            checkbox.toggle.isEnabled = node["enabled"] as? Bool ?? false
            checkbox.toggle.accessibilityLabel = node["name"] as? String ?? node["text"] as? String
            checkbox.toggle.accessibilityHint = node["description"] as? String
            checkbox.caption.text = node["text"] as? String
            checkbox.caption.textColor = color(style["foreground"])
        }
    }

    private func reuseKey(_ node: [String: Any]) -> String? {
        guard let kind = node["kind"] as? String,
              ["text", "button", "input", "textarea", "checkbox", "switch", "select", "image"].contains(kind) else { return nil }
        return kind == "input" && node["password"] as? Bool == true ? "secure-input" : kind
    }

    private func takeView(_ node: [String: Any], id: String? = nil) throws -> UIView {
        if let id = id ?? node["id"] as? String, let pending = preboundViews.removeValue(forKey: id) {
            if pending.key == reuseKey(node) { return pending.view }
            recycle(pending.view, key: pending.key)
        }
        if let key = reuseKey(node), let view = recycledViews[key]?.popLast() {
            recycledViewCount -= 1; return view
        }
        return try make(node)
    }

    private func recycle(_ view: UIView, key: String) {
        view.endEditing(true); view.removeFromSuperview()
        (view as? NativeSelect)?.retire(); (view as? JadixImageView)?.dispose()
        view.accessibilityLabel = nil; view.accessibilityHint = nil; view.accessibilityValue = nil
        view.accessibilityElements = nil; view.accessibilityTraits = []
        view.layer.removeAllAnimations(); view.transform = .identity; view.alpha = 1; view.isHidden = false
        (view as? JadixButton)?.keyboardInput.reset()
        (view as? JadixContainer)?.keyboardInput.reset()
        view.shouldGroupAccessibilityChildren = false
        if let input = view as? UITextField { input.text = ""; input.attributedText = nil; input.undoManager?.removeAllActions() }
        if let input = view as? UITextView { input.text = ""; input.selectedRange = NSRange(location: 0, length: 0); input.undoManager?.removeAllActions() }
        if let label = view as? UILabel { label.attributedText = nil; label.text = nil }
        if let button = view as? UIButton { button.setAttributedTitle(nil, for: .normal); button.setTitle(nil, for: .normal) }
        guard recycledViewCount < 128, (recycledViews[key]?.count ?? 0) < 32 else { return }
        recycledViews[key, default: []].append(view); recycledViewCount += 1
    }

    func measure(_ json: String) throws -> CGSize {
        let request = try object(json)
        guard let node = request["node"] as? [String: Any], let width = request["maxWidth"] as? Double,
              let height = request["maxHeight"] as? Double else { throw NativeError.invalid("invalid native measure") }
        measuredBaseline = .nan
        func constrained(_ size: CGSize) -> CGSize {
            CGSize(width: request["widthMode"] as? String == "exact" ? width : min(width, max(0, Double(size.width))),
                   height: request["heightMode"] as? String == "exact" ? height : min(height, max(0, Double(size.height))))
        }
        if node["kind"] as? String == "hosted" {
            let spec = node["specification"] as? [String: Any] ?? [:]
            let kind = spec["type"] as? String
            let font = UIFont.preferredFont(forTextStyle: .body, compatibleWith: traitCollection)
            if kind == "tab_bar" {
                if spec["hidden"] as? Bool == true { return constrained(.zero) }
                let control = JadixTabBar(); control.update(spec) { _ in }
                return constrained(control.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)))
            }
            if kind == "menu_button" || kind == "segments" {
                let items = spec["items"] as? [[String: Any]] ?? []
                let label = spec["label"] as? String ?? items.compactMap { $0["label"] as? String }.joined(separator: "    ")
                let symbols = items.filter { $0["symbol"] is String }.count
                return constrained(CGSize(width: (label as NSString).size(withAttributes: [.font: font]).width + 36 + CGFloat(symbols * 20), height: max(32, font.lineHeight + 12)))
            }
            return constrained(CGSize(width: 320, height: 180))
        }
        // Measurement never starts image IO. Publication owns the decode and
        // invalidates layout when the source's original dimensions arrive.
        if node["kind"] as? String == "image" {
            guard let id = request["id"] as? String, let source = node["source"] as? String,
                  let size = (views[id] as? JadixImageView)?.originalSize(for: source), size.width > 0, size.height > 0 else { return constrained(.zero) }
            let style = node["style"] as? [String: Any]
            let scale = (style?["width"] as? Double).map { min(width, $0) / Double(size.width) }
                ?? (style?["height"] as? Double).map { min(height, $0) / Double(size.height) } ?? 1
            let fittingScale = min(scale, width / Double(size.width), height / Double(size.height))
            return constrained(CGSize(width: Double(size.width) * fittingScale, height: Double(size.height) * fittingScale))
        }
        let id = request["id"] as? String, key = reuseKey(node)
        let view: UIView
        if let id, let pending = preboundViews[id], pending.key == key { view = pending.view }
        else {
            view = try takeView(node, id: id)
            if let id, let key, views[id] == nil, preboundViews.count < 256 { preboundViews[id] = (key, view) }
        }
        defer {
            if let key, id.flatMap({ preboundViews[$0]?.view }) !== view { recycle(view, key: key) }
        }
        try apply(node, to: view)
        let size = constrained(view.sizeThatFits(CGSize(width: width, height: height)))
        if let label = view as? JadixLabel { measuredBaseline = label.firstBaseline(in: size) }
        else {
            let font = (view as? UITextField)?.font ?? (view as? UITextView)?.font ?? (view as? UIButton)?.titleLabel?.font
            let baseline = font.map { Double($0.ascender + ((view as? UITextView)?.textContainerInset.top ?? 0)) } ?? .nan
            if baseline > 0 && baseline <= size.height { measuredBaseline = baseline }
        }
        return size
    }

    fileprivate func publish(_ json: String) throws { try publish(try object(json)) }

    private func publish(_ snapshot: [String: Any]) throws {
        guard snapshot["protocol"] as? Int == 1,
              let token = snapshot["root"] as? String, root == 0 || token == String(root) else {
            throw NativeError.invalid("invalid native snapshot")
        }
        if try publishService(snapshot) { return }
        guard let nodes = snapshot["nodes"] as? [[String: Any]] else { throw NativeError.invalid("missing native nodes") }
        publishing = true
        defer { publishing = false }
        viewportSnapshot = snapshot
        let materialized = materializedIDs(nodes, focused: snapshot["focused"] as? String)
        let live = Set(nodes.compactMap { $0["id"] as? String })
        // Children leave first. Descriptors/edit revisions remain owned by the logical tree.
        for node in nodes.reversed() {
            guard let id = node["id"] as? String, !materialized.contains(id),
                  let view = views.removeValue(forKey: id) else { continue }
            viewAppearances.removeValue(forKey: id)?.remove()
            interactions.removeValue(forKey: id)?.dispose()
            identities.removeValue(forKey: ObjectIdentifier(view)); view.removeFromSuperview()
            (view as? JadixImageView)?.dispose()
            if let previous = descriptors[id], let key = reuseKey(previous) { recycle(view, key: key) }
        }
        var indices: [ObjectIdentifier: Int] = [:]
        for node in nodes {
            guard let id = node["id"] as? String, let frame = node["frame"] as? [Double], frame.count == 4 else {
                throw NativeError.invalid("invalid native node")
            }
            guard materialized.contains(id) else { descriptors[id] = node; continue }
            let newlyMaterialized = views[id] == nil
            if let previous = descriptors[id], previous["kind"] as? String != node["kind"] as? String,
               let previousView = views.removeValue(forKey: id) {
                if let popover = previousView as? NativePopover, popover.reportedOpen { onPopoverEvent?(id, .closed) }
                (previousView as? JadixHostedView)?.dispose(); (previousView as? JadixImageView)?.dispose()
                (previousView as? JadixScrollView)?.retire(); (previousView as? NativeSelect)?.retire(); (previousView as? NativePopover)?.retire(); (previousView as? NativeModal)?.hide()
                interactions.removeValue(forKey: id)?.dispose()
                viewAppearances.removeValue(forKey: id)?.remove(); identities.removeValue(forKey: ObjectIdentifier(previousView))
                previousView.removeFromSuperview()
                if let previous = descriptors[id], let key = reuseKey(previous) { recycle(previousView, key: key) }
                descriptors.removeValue(forKey: id)
            }
            let view = try views[id] ?? takeView(node)
            views[id] = view
            identities[ObjectIdentifier(view)] = id
            if ["box", "text", "image", "button"].contains(node["kind"] as? String ?? "") {
                let input = interactions[id] ?? JadixInteractionInput(); interactions[id] = input
                input.configure(node["interaction"] as? [String: Any] ?? [:], view: view, button: node["kind"] as? String == "button") { [weak self, weak view] payload in
                    guard let self, let view, self.views[id] === view else { return }; self.emit(view, payload)
                }
            }
            if let keyboard = (view as? JadixButton)?.keyboardInput ?? (view as? JadixContainer)?.keyboardInput {
                keyboard.activationKeys = node["kind"] as? String == "button" ? [44] : []
                if node["kind"] as? String == "button", (node["navigationKeys"] as? [String])?.contains("enter") != true {
                    keyboard.activationKeys.formUnion([40, 88])
                }
                keyboard.configure(node["keyboard"] as? [String: Any] ?? [:], view: view) { [weak self, weak view] payload in
                    guard let self, let view, self.views[id] === view else { return }
                    self.emit(view, payload)
                }
            }
            if let select = view as? NativeSelect, select.onSelect == nil {
                select.onSelect = { [weak self, weak select] value in
                    guard let self = self, let select = select, !self.publishing,
                          self.views[id] === select, self.descriptors[id] != nil else { return }
                    let (revision, overflow) = self.revisions[id, default: 0].addingReportingOverflow(1)
                    guard !overflow else { return }
                    self.revisions[id] = revision
                    self.emit(select, ["kind": "select", "value": value, "revision": String(revision)])
                }
            }
            if let modal = view as? NativeModal {
                modal.moveFocus = { [weak self] in guard let self, !self.hasMarkedText else { return false }; return self.moveTabFocus(backwards: $0) }
                modal.dismissDialog = { [weak self, weak modal] backdrop in
                    guard let self = self, let modal = modal, !self.publishing, !self.hasMarkedText,
                          let node = self.descriptors[id], !backdrop || node["closeOnOverlayClick"] as? Bool == true else { return }
                    let (revision, overflow) = self.revisions[id, default: 0].addingReportingOverflow(1)
                    guard !overflow else { return }
                    self.revisions[id] = revision
                    self.emit(modal, ["kind": "dismiss", "backdrop": backdrop, "revision": String(revision)])
                }
            }
            if let popover = view as? NativePopover {
                popover.moveFocus = { [weak self] in guard let self, !self.hasMarkedText else { return false }; return self.moveTabFocus(backwards: $0) }
                popover.notify = { [weak self, weak popover] event in
                    guard let self, let popover, self.root != 0, self.views[id] === popover else { return }
                    switch event {
                    case .opened: self.emit(popover, ["kind": "popoverLifecycle", "open": true])
                    case .closed: self.emit(popover, ["kind": "popoverLifecycle", "open": false])
                    case .positionChanged(let position):
                        var payload: [String: Any] = ["kind": "popoverPosition"]
                        if let position { payload["position"] = ["frame": ["x": position.frame.minX, "y": position.frame.minY, "width": position.frame.width, "height": position.frame.height], "placement": position.placement, "availableWidth": position.availableSize.width, "availableHeight": position.availableSize.height] as [String: Any] }
                        self.emit(popover, payload)
                    case .dismissRequested: break
                    }
                    self.onPopoverEvent?(id, event)
                }
                popover.acceptsDismiss = { [weak self, weak popover] in
                    guard let self = self, let popover = popover else { return false }
                    return !self.publishing && !self.hasMarkedText && self.root != 0 && self.views[id] === popover
                        && self.popoverOrder.last == id && self.eventVisible(id)
                }
                popover.dismissPopover = { [weak self, weak popover] backdrop in
                    guard let self = self, let popover = popover, self.views[id] === popover,
                          let node = self.descriptors[id], !backdrop || node["closeOnOutsideClick"] as? Bool == true else { return }
                    let (revision, overflow) = self.revisions[id, default: 0].addingReportingOverflow(1)
                    guard !overflow else { return }
                    self.revisions[id] = revision
                    self.emit(popover, ["kind": "dismiss", "backdrop": backdrop, "revision": String(revision)])
                }
                popover.cancelPopover = { [weak self, weak popover] backdrop in
                    guard let self, let popover, self.views[id] === popover else { return }
                    self.emit(popover, ["kind": "popoverCancel", "backdrop": backdrop])
                }
            }
            let parent: UIView
            var origin = [0.0, 0.0]
            if let parentID = node["parent"] as? String, let parentView = views[parentID],
               let parentFrame = descriptors[parentID]?["frame"] as? [Double] {
                parent = (parentView as? NativeModal)?.content ?? (parentView as? NativePopover)?.content ?? (parentView as? JadixScrollView)?.document ?? (parentView as? JadixHostedView)?.content ?? parentView; origin = [parentFrame[0], parentFrame[1]]
            } else if node["parent"] is NSNull { parent = self }
            else { throw NativeError.invalid("invalid native parent") }
            let index = indices[ObjectIdentifier(parent), default: 0]
            indices[ObjectIdentifier(parent)] = index + 1
            if view.superview !== parent || parent.subviews.firstIndex(of: view) != index { parent.insertSubview(view, at: index) }
            if newlyMaterialized || descriptors[id].map({ NSDictionary(dictionary: $0).isEqual(NSDictionary(dictionary: node)) }) != true {
                let preserveComposition = descriptors[id]?["text"] as? String == node["text"] as? String && node["enabled"] as? Bool == true
                try apply(node, to: view, preserveComposition: preserveComposition)
            }
            let nextFrame = view is NativeModal || view is NativePopover ? .zero : CGRect(x: frame[0] - origin[0], y: frame[1] - origin[1], width: frame[2], height: frame[3])
            if view.frame != nextFrame {
                view.frame = nextFrame
                view.setNeedsLayout()
                (view as? UIButton)?.titleLabel?.setNeedsLayout()
            }
            if let hosted = view as? JadixHostedView { bindHosted(hosted, id: id, node: node) }
            if let image = view as? JadixImageView {
                image.onLoadEvent = { [weak self, weak image] event in
                    guard let self, let image, self.views[id] === image else { return }; self.emit(image, event)
                }
                image.onOriginalSizeChange = { [weak self] in
                    guard let self, !self.imageLayoutPending else { return }; self.imageLayoutPending = true
                    DispatchQueue.main.async { [weak self] in guard let self else { return }; self.imageLayoutPending = false; self.invalidateEnvironment() }
                }
                image.refreshBackingDensity()
            }
            if !(view is NativePopover), let style = node["style"] as? [String: Any], let appearance = style["appearance"] as? [String: Any] {
                let renderer = viewAppearances[id] ?? JadixViewAppearance((view as? NativeModal)?.content ?? view); viewAppearances[id] = renderer
                renderer.update(appearance, background: color(style["background"]))
            } else { viewAppearances.removeValue(forKey: id)?.remove() }
            if let modal = view as? NativeModal, let style = node["style"] as? [String: Any], style["appearance"] == nil {
                modal.content.backgroundColor = color(style["background"])
            }
            if let value = node["editRevision"] as? String, let revision = UInt64(value) { revisions[id] = max(revisions[id, default: 0], revision) }
            descriptors[id] = node
        }
        for id in popoverOrder.reversed() where !live.contains(id) {
            (views[id] as? NativePopover)?.retire()
        }
        popoverOrder.removeAll(where: { !live.contains($0) })
        for id in modalOrder.reversed() where !live.contains(id) { (views[id] as? NativeModal)?.hide() }
        for id in Array(descriptors.keys) where !live.contains(id) {
            if let view = views.removeValue(forKey: id) {
                if let popover = view as? NativePopover, popover.reportedOpen { onPopoverEvent?(id, .closed) }
                (view as? JadixHostedView)?.dispose()
                (view as? JadixImageView)?.dispose()
                viewAppearances.removeValue(forKey: id)?.remove()
                (view as? JadixScrollView)?.retire()
                (view as? NativeSelect)?.retire()
                (view as? NativePopover)?.retire()
                (view as? NativeModal)?.hide()
                interactions.removeValue(forKey: id)?.dispose()
                identities.removeValue(forKey: ObjectIdentifier(view)); view.removeFromSuperview()
                if let previous = descriptors[id], let key = reuseKey(previous) { recycle(view, key: key) }
            }
            descriptors.removeValue(forKey: id); revisions.removeValue(forKey: id)
        }
        for id in Array(preboundViews.keys) where !live.contains(id) || !materialized.contains(id) {
            if let pending = preboundViews.removeValue(forKey: id) { recycle(pending.1, key: pending.0) }
        }
        // Full logical geometry survives eviction; aggregate once for all scroll owners.
        var childExtents: [String: CGPoint] = [:]
        for node in nodes {
            guard let parent = node["parent"] as? String, let frame = node["frame"] as? [Double], frame.count == 4 else { continue }
            let old = childExtents[parent] ?? CGPoint(x: -CGFloat.greatestFiniteMagnitude, y: -CGFloat.greatestFiniteMagnitude)
            childExtents[parent] = CGPoint(x: max(old.x, frame[0] + frame[2]), y: max(old.y, frame[1] + frame[3]))
        }
        for (id, view) in views {
            (view as? JadixHostedView)?.setNeedsLayout()
            if let scroll = view as? JadixScrollView {
                if let origin = descriptors[id]?["frame"] as? [Double] {
                    let extent = childExtents[id] ?? CGPoint(x: origin[0], y: origin[1])
                    scroll.layoutExtent = CGSize(width: max(0, extent.x - origin[0]), height: max(0, extent.y - origin[1]))
                }
                scroll.applyStyle(descriptors[id]?["style"] as? [String: Any])
            }
        }
        var opened: [String] = []
        for node in nodes where node["kind"] as? String == "dialog" && node["open"] as? Bool == true {
            guard let id = node["id"] as? String else { continue }
            var ancestor = node
            var visible = true
            while let parent = ancestor["parent"] as? String, let next = descriptors[parent] {
                ancestor = next
                if ["dialog", "popover"].contains(ancestor["kind"] as? String ?? "") && ancestor["open"] as? Bool != true { visible = false; break }
            }
            if visible { opened.append(id) }
        }
        let prefix = zip(modalOrder, opened).prefix(while: { $0.0 == $0.1 }).count
        for id in modalOrder.dropFirst(prefix).reversed() { (views[id] as? NativeModal)?.hide() }
        modalOrder = opened
        for id in opened {
            guard let modal = views[id] as? NativeModal, let frame = descriptors[id]?["frame"] as? [Double] else { continue }
            try modal.show(in: self, frame: CGRect(x: frame[0], y: frame[1], width: frame[2], height: frame[3]),
                           name: descriptors[id]?["name"] as? String ?? "Dialog",
                           description: descriptors[id]?["description"] as? String)
            if let renderer = viewAppearances[id] { renderer.render(renderer.current) }
        }
        let visiblePopovers = nodes.compactMap { node -> String? in
            guard node["kind"] as? String == "popover", node["open"] as? Bool == true,
                  let id = node["id"] as? String, eventVisible(id) else { return nil }
            return id
        }
        for id in popoverOrder.reversed() where !visiblePopovers.contains(id) { (views[id] as? NativePopover)?.hide(restore: true) }
        popoverOrder.removeAll(where: { !visiblePopovers.contains($0) })
        for id in visiblePopovers where !popoverOrder.contains(id) { popoverOrder.append(id) }
        try updatePopovers()
        schedulePopovers()
        var gridMembers: [String: [(String, UIView, [String: Any])]] = [:]
        for child in nodes {
            guard let childID = child["id"] as? String, let view = views[childID], eventVisible(childID),
                  child["cell"] is [String: Any] || child["columnHeader"] is [String: Any] else { continue }
            var parent = child["parent"] as? String
            while let key = parent, let ancestor = descriptors[key] {
                if ancestor["grid"] is [String: Any] { gridMembers[key, default: []].append((childID, view, child)); break }
                parent = ancestor["parent"] as? String
            }
        }
        for node in nodes {
            guard let id = node["id"] as? String, let container = views[id] as? JadixContainer else { continue }
            container.configureGrid(node["grid"] as? [String: Any], entries: gridMembers[id] ?? [])
        }
        nodeOrder = nodes.compactMap { $0["id"] as? String }
        var activeDescendants: [String: String] = [:]
        for node in nodes {
            guard let id = node["id"] as? String, let view = views[id] else { continue }
            let relations = node["accessibilityRelations"] as? [String: Any]
            let controlled = (relations?["controls"] as? [[String: String]] ?? []).compactMap { target -> String? in
                guard target["root"] == token, let targetID = target["id"], live.contains(targetID), eventVisible(targetID) else { return nil }
                return targetID
            }
            // UIKit has no ARIA-controls setter. Expose live, root-scoped
            // destinations as VoiceOver actions, with weak ownership.
            view.accessibilityCustomActions = controlled.compactMap { targetID in
                guard let target = views[targetID] else { return nil }
                return UIAccessibilityCustomAction(name: target.accessibilityLabel ?? NSLocalizedString("Show content", comment: "Accessibility relationship action")) { [weak self, weak target] _ in
                    guard let self, let target, self.views[targetID] === target, self.eventVisible(targetID) else { return false }
                    self.scheduleFocus(targetID)
                    UIAccessibility.post(notification: .layoutChanged, argument: target); return true
                }
            }
            if let target = relations?["activeDescendant"] as? [String: String], target["root"] == token,
               let targetID = target["id"], live.contains(targetID), eventVisible(targetID) {
                activeDescendants[id] = targetID
                if accessibilityActiveDescendants[id] != targetID, nodeID(for: focusedDescendant) == id {
                    let expectedRoot = root
                    DispatchQueue.main.async { [weak self] in
                        guard let self, self.root == expectedRoot, self.accessibilityActiveDescendants[id] == targetID,
                              self.eventVisible(targetID), let target = self.views[targetID] else { return }
                        UIAccessibility.post(notification: .layoutChanged, argument: target)
                    }
                }
            }
        }
        accessibilityActiveDescendants = activeDescendants
        if snapshot["focusChanged"] as? Bool == true { scheduleFocus(snapshot["focused"] as? String) }
    }

    /// Only stateless presentation nodes are evicted. Interactive controls retain
    /// their native undo/selection/focus state; VoiceOver explores the complete tree.
    private func materializedIDs(_ nodes: [[String: Any]], focused: String?) -> Set<String> {
        let all = Set(nodes.compactMap { $0["id"] as? String })
        guard !UIAccessibility.isVoiceOverRunning, nodes.contains(where: { $0["kind"] as? String == "scroll" }) else { return all }
        var indices: [String: Int] = [:], protected: [Bool] = [], input: [[String: Any]] = []
        let identity = [1.0,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1]
        for (index, node) in nodes.enumerated() {
            guard let id = node["id"] as? String, let frame = node["frame"] as? [Double], frame.count == 4 else { return all }
            let parent = (node["parent"] as? String).flatMap { indices[$0] }
            if node["parent"] is String && parent == nil { return all }
            let kind = node["kind"] as? String ?? "", style = node["style"] as? [String: Any]
            let appearance = style?["appearance"] as? [String: Any]
            let transformed = (appearance?["transform"] as? [Double]).map { $0 != identity } ?? false
            let animated = ["animation", "transition"].contains { appearance?[$0] != nil && !(appearance?[$0] is NSNull) }
            // Zoom and effects can project pixels beyond document-space bounds.
            let zoomed = kind == "scroll" && style?["scroll_zoom"] != nil && !(style?["scroll_zoom"] is NSNull)
            let effects = ["shadows", "filters", "backdrop_filters"].contains { !(appearance?[$0] as? [Any] ?? []).isEmpty }
                || (appearance?["outline_width"] as? Double ?? 0) != 0
            let preserveSubtree = (parent.map { protected[$0] } ?? false) || transformed || animated || zoomed || effects
                || ["hosted", "dialog", "popover"].contains(kind) || node["grid"] is [String: Any]
                || ["grid", "tree", "listbox"].contains(node["role"] as? String ?? "")
            protected.append(preserveSubtree)
            var viewport: Any = NSNull()
            if kind == "scroll" {
                if let scroll = views[id] as? JadixScrollView {
                    var value = scroll.cullingViewport()
                    value["size"] = [frame[2] / Double(scroll.zoomScale), frame[3] / Double(scroll.zoomScale)]
                    viewport = value
                } else {
                    let zoom = style?["scroll_zoom"] as? [String: Any], scale = zoom?["initial"] as? Double ?? 1
                    viewport = ["offset": style?["scroll_to"] as? [Double] ?? [0,0],
                                "size": [frame[2] / scale, frame[3] / scale], "velocity": [0,0], "elapsed": 0]
                }
            }
            let interactive = (node["interaction"] as? [String: Any] ?? [:]).values.contains {
                $0 is String || (($0 as? NSNumber)?.doubleValue ?? 0) > 0
            }
            let pinned = interactive || preserveSubtree || !["box", "text", "image"].contains(kind) || node["tabStop"] as? Bool == true
                || id == focused || views[id]?.isFirstResponder == true
                || views[id].map { !($0.layer.animationKeys() ?? []).isEmpty } == true
            input.append(["parent": parent.map { $0 as Any } ?? NSNull(), "frame": frame, "viewport": viewport, "pinned": pinned])
            indices[id] = index
        }
        guard let data = try? JSONSerialization.data(withJSONObject: input), data.count <= 32 * 1024 * 1024 else { return all }
        let raw = String(decoding: data, as: UTF8.self).withCString { jadix_native_viewport_plan($0) }
        guard let raw else { return all }; defer { jadix_native_string_free(raw) }
        guard let data = String(cString: raw).data(using: .utf8), let plan = try? JSONSerialization.jsonObject(with: data) as? [Bool], plan.count == nodes.count else { return all }
        return Set(nodes.enumerated().compactMap { plan[$0.offset] ? $0.element["id"] as? String : nil })
    }

    private func scheduleCulling() {
        guard viewportSnapshot != nil, root != 0 else { return }
        cullingGeneration &+= 1
        let generation = cullingGeneration, mounted = root
        // The last pass releases the bounded urgent lead after scrolling settles.
        for delay in [0.0, 0.2, 0.6] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
                guard let self, self.root == mounted, self.cullingGeneration == generation,
                      !self.publishing, var snapshot = self.viewportSnapshot else { return }
                snapshot["focusChanged"] = false
                do { try self.publish(snapshot) } catch { self.onError?(error) }
            }
        }
    }

    private func updatePopovers() throws {
        for id in popoverOrder {
            guard let popover = views[id] as? NativePopover else { continue }
            guard eventVisible(id), let node = descriptors[id], let anchorID = node["anchor"] as? String,
                  let anchor = views[anchorID] else { popover.suspend(); continue }
            try popover.show(in: self, anchor: anchor, node: node)
        }
    }

    private func schedulePopovers() {
        guard !popoverUpdatePending, root != 0, !popoverOrder.isEmpty else { return }
        popoverUpdatePending = true
        let expectedRoot = root
        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            self.popoverUpdatePending = false
            guard self.root == expectedRoot, self.window != nil else { return }
            do { try self.updatePopovers() } catch { self.onError?(error) }
        }
    }

    func popover(named name: String) -> NativePopover? {
        for (id, node) in descriptors where node["kind"] as? String == "popover" && node["name"] as? String == name {
            return views[id] as? NativePopover
        }
        return nil
    }

    fileprivate func clear() {
        handoff.clear()
        for input in interactions.values { input.dispose() }; interactions.removeAll()
        viewportSnapshot = nil; cullingGeneration &+= 1
        publishing = true
        defer { publishing = false }
        for id in popoverOrder.reversed() { (views[id] as? NativePopover)?.retire() }
        for (id, view) in views {
            if let popover = view as? NativePopover, popover.reportedOpen { onPopoverEvent?(id, .closed) }
            (view as? NativePopover)?.retire()
        }
        popoverOrder.removeAll()
        for id in modalOrder.reversed() { (views[id] as? NativeModal)?.hide() }
        for view in views.values { (view as? JadixScrollView)?.retire(); (view as? NativeSelect)?.retire() }
        modalOrder.removeAll()
        for view in views.values { (view as? JadixHostedView)?.dispose(); (view as? JadixImageView)?.dispose() }
        for renderer in viewAppearances.values { renderer.remove() }; viewAppearances.removeAll()
        for pending in preboundViews.values { pending.view.endEditing(true); (pending.view as? JadixImageView)?.dispose() }
        preboundViews.removeAll(); recycledViews.removeAll(); recycledViewCount = 0
        appliedContentSizeCategory = nil
        subviews.forEach { $0.removeFromSuperview() }
        views.removeAll(); descriptors.removeAll(); identities.removeAll(); revisions.removeAll()
        nodeOrder.removeAll(); accessibilityActiveDescendants.removeAll()
    }

    private func eventVisible(_ id: String) -> Bool {
        var current: String? = id
        var inModal = modalOrder.isEmpty
        while let key = current, let node = descriptors[key] {
            if ["dialog", "popover"].contains(node["kind"] as? String ?? "") && node["open"] as? Bool != true { return false }
            if key == modalOrder.last { inModal = true }
            current = node["parent"] as? String
        }
        return inModal
    }

    private func emit(_ view: UIView, _ payload: [String: Any]) {
        let cancelled = payload["kind"] as? String == "interaction" && (payload["event"] as? [String: Any])?["phase"] as? String == "cancelled"
        guard !publishing, root != 0, let id = identities[ObjectIdentifier(view)],
              cancelled || payload["kind"] as? String == "blur" || eventVisible(id) else { return }
        do {
            let data = try JSONSerialization.data(withJSONObject: ["root": String(root), "node": id, "payload": payload])
            let json = String(decoding: data, as: UTF8.self)
            let state = try json.withCString { try reply(jadix_native_event(root, $0)) }
            onStateChange?(state)
        } catch {
            if let descriptor = descriptors[id] {
                publishing = true
                defer { publishing = false }
                try? apply(descriptor, to: view)
            }
            onError?(error)
        }
    }

    @objc private func activate(_ button: UIButton) { emit(button, ["kind": "activate"]) }
    @objc private func focused(_ input: UITextField) {
        emit(input, ["kind": "focus"])
        revealFocusedControl()
    }
    @objc private func toggled(_ checkbox: NativeCheckbox) {
        guard !publishing, let id = identities[ObjectIdentifier(checkbox)] else { return }
        let revision = revisions[id, default: 0] + 1
        revisions[id] = revision
        emit(checkbox, ["kind": "toggle", "checked": checkbox.toggle.isOn, "revision": String(revision)])
    }
    @objc private func edited(_ input: UITextField) {
        guard !publishing, let id = identities[ObjectIdentifier(input)], let selection = input.selectedTextRange else { return }
        let (revision, overflow) = revisions[id, default: 0].addingReportingOverflow(1)
        guard !overflow else { return }
        revisions[id] = revision
        let start = input.offset(from: input.beginningOfDocument, to: selection.start)
        let end = input.offset(from: input.beginningOfDocument, to: selection.end)
        var payload: [String: Any] = ["kind": "edit", "value": input.text ?? "", "revision": String(revision), "selection": [min(start, end), max(start, end)], "composing": NSNull()]
        if let marked = input.markedTextRange {
            payload["composing"] = [input.offset(from: input.beginningOfDocument, to: marked.start), input.offset(from: input.beginningOfDocument, to: marked.end)]
        }
        emit(input, payload)
    }
    public func textFieldDidChangeSelection(_ textField: UITextField) { edited(textField) }
    public func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        guard !publishing, root != 0, textField.isEnabled, textField.markedTextRange == nil,
              let id = identities[ObjectIdentifier(textField)], views[id] === textField, eventVisible(id) else { return false }
        let expectedRoot = root
        edited(textField)
        guard root == expectedRoot, views[id] === textField, textField.isEnabled,
              textField.markedTextRange == nil else { return false }
        let (revision, overflow) = revisions[id, default: 0].addingReportingOverflow(1)
        guard !overflow else { return false }
        revisions[id] = revision
        emit(textField, ["kind": "submit", "revision": String(revision)])
        return false
    }
    public func textFieldDidEndEditing(_ textField: UITextField) {
        emit(textField, ["kind": "blur"])
    }
    public func textViewDidBeginEditing(_ textView: UITextView) {
        emit(textView, ["kind": "focus"])
        revealFocusedControl()
    }
    public func textViewDidChange(_ textView: UITextView) { multilineEdited(textView) }
    public func textViewDidEndEditing(_ textView: UITextView) {
        emit(textView, ["kind": "blur"])
    }
    public func textViewDidChangeSelection(_ textView: UITextView) { multilineEdited(textView) }
    private func multilineEdited(_ input: UITextView) {
        guard !publishing, let id = identities[ObjectIdentifier(input)] else { return }
        let revision = revisions[id, default: 0] + 1
        revisions[id] = revision
        var payload: [String: Any] = ["kind": "edit", "value": input.text ?? "", "revision": String(revision),
            "selection": [input.selectedRange.location, input.selectedRange.location + input.selectedRange.length], "composing": NSNull()]
        if let marked = input.markedTextRange {
            payload["composing"] = [input.offset(from: input.beginningOfDocument, to: marked.start), input.offset(from: input.beginningOfDocument, to: marked.end)]
        }
        emit(input, payload)
        input.setNeedsLayout()
    }
    public func nativeScroll(nodeID: String) -> JadixScrollHandle? {
        guard root != 0, let scroll = views[nodeID] as? JadixScrollView, scroll.observers.count < 64 else { return nil }
        let mounted = root
        return JadixScrollHandle(scroll) { [weak self, weak scroll] in
            guard let self, let scroll else { return false }
            return self.root == mounted && self.views[nodeID] === scroll
        }
    }

    public func viewForZooming(in scrollView: UIScrollView) -> UIView? { (scrollView as? JadixScrollView)?.document }
    public func scrollViewDidZoom(_ scrollView: UIScrollView) { scrollViewDidScroll(scrollView) }
    public func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard let scroll = scrollView as? JadixScrollView, let id = identities[ObjectIdentifier(scroll)], !scroll.reportPending else { return }
        scroll.reportPending = true
        DispatchQueue.main.async { [weak self, weak scroll] in
            guard let scroll else { return }; scroll.reportPending = false
            guard let self, self.root != 0, self.views[id] === scroll else { return }
            let offset = scroll.logicalOffset, viewport = scroll.viewport, scale = scroll.zoomScale
            guard offset != scroll.reportedOffset || viewport != scroll.reportedViewport || scale != scroll.reportedScale else { return }
            let changed = scale != scroll.reportedScale
            scroll.reportedOffset = offset; scroll.reportedViewport = viewport; scroll.reportedScale = scale
            self.emit(scroll, ["kind": "scroll", "x": offset.x, "y": offset.y, "viewport": [viewport.width, viewport.height]])
            if changed, self.views[id] === scroll {
                self.emit(scroll, ["kind": "scrollZoom", "scale": scale, "x": offset.x, "y": offset.y, "width": viewport.width, "height": viewport.height])
                for image in self.views.values.compactMap({ $0 as? JadixImageView }) where image.isDescendant(of: scroll) { image.refreshBackingDensity() }
            }
            for callback in Array(scroll.observers.values) { callback(offset, viewport, scale) }
            self.schedulePopovers()
            self.scheduleCulling()
        }
    }

}

extension JadixNativeView {

    private func publishService(_ snapshot: [String: Any]) throws -> Bool {
        if let address = snapshot["appAddress"] {
            guard address is NSNull || address is String else { throw NativeError.invalid("invalid app address") }
            handoff.setAddress(address as? String); refreshHandoff(); return true
        }
        if let command = snapshot["imageCache"] as? [String: Any] {
            guard root != 0, let bytes = command["bytes"] as? Int, (0...33554432).contains(bytes),
                  let entries = command["entries"] as? Int, (0...128).contains(entries),
                  let age = command["age"] as? Double, age.isFinite, (0...86400).contains(age) else { throw NativeError.invalid("invalid image cache budget") }
            let loader = imageLoader()
            Task { @MainActor in await loader.configureCache(bytes: bytes, entries: entries, age: age) }
            return true
        }
        if let command = snapshot["desktopService"] as? [String: Any] { guard root != 0 else { return true }; desktopServices.handle(command); return true }
        if let command = snapshot["fetch"] as? [String: Any] {
            guard root != 0 else { throw NativeError.invalid("unmounted HTTP service") }
            let mountedRoot = root
            fetchService.event = { [weak self] id, event in
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.root == mountedRoot,
                          let data = try? JSONSerialization.data(withJSONObject: event), data.count <= 400 * 1024,
                          let json = String(data: data, encoding: .utf8) else { return }
                    json.withCString { _ = jadix_native_fetch_event(mountedRoot, id, $0) }
                }
            }
            fetchService.handle(command); return true
        }
        if let command = snapshot["speech"] as? [String: Any] {
            guard root != 0 else { throw NativeError.invalid("unmounted speech service") }
            let mountedRoot = root
            speechService.event = { [weak self] id, event in
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.root == mountedRoot,
                          let data = try? JSONSerialization.data(withJSONObject: event), data.count <= 70 * 1024,
                          let json = String(data: data, encoding: .utf8) else { return }
                    json.withCString { _ = jadix_native_speech_event(mountedRoot, id, $0) }
                }
            }
            speechService.handle(command); return true
        }
        if let commands = snapshot["audio"] as? [[String: Any]] {
            guard root != 0 else { throw NativeError.invalid("unmounted audio service") }
            let mountedRoot = root
            audioService.event = { [weak self] id, event in
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.root == mountedRoot,
                          let data = try? JSONSerialization.data(withJSONObject: event), data.count <= 16 * 1024,
                          let json = String(data: data, encoding: .utf8) else { return }
                    json.withCString { _ = jadix_native_audio_event(mountedRoot, id, $0) }
                }
            }
            audioService.handle(commands); return true
        }
        if let command = snapshot["webResource"] as? [String: Any] {
            guard let serial = command["request"] as? String, let request = UInt64(serial),
                  let answer = command["answer"] as? [String: Any] else { throw NativeError.invalid("invalid web resource response") }
            guard let reply = webResourceReplies.removeValue(forKey: request) else { return true }
            if answer["error"] is String { reply(.failure(URLError(.cannotLoadFromNetwork))) }
            else if let status = answer["status"] as? Int, let mime = answer["mime"] as? String,
                    let encoded = answer["bytes"] as? String, encoded.utf8.count <= 44_739_244,
                    let bytes = Data(base64Encoded: encoded), bytes.count <= 32 * 1024 * 1024 {
                reply(.success(JadixWebResourceResponse(status: status, mime: mime, bytes: bytes)))
            } else { reply(.failure(URLError(.badServerResponse))) }
            return true
        }
        if let command = snapshot["webEvaluate"] as? [String: Any] {
            guard root != 0, let id = command["node"] as? String,
                  let hosted = views[id] as? JadixHostedView,
                  let serial = command["request"] as? String, let request = UInt64(serial),
                  let script = command["script"] as? String else { throw NativeError.invalid("invalid web evaluation") }
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
            return true
        }
        if let command = snapshot["stopScroll"] as? [String: Any] {
            guard let id = command["node"] as? String, let scroll = views[id] as? JadixScrollView else { throw NativeError.invalid("invalid scroll stop") }
            scroll.stop(); return true
        }
        if let command = snapshot["scrollTo"] as? [String: Any] {
            guard let id = command["node"] as? String, let scroll = views[id] as? JadixScrollView,
                  let offset = command["offset"] as? [Double], offset.count == 2, offset.allSatisfy({ $0.isFinite && $0 >= 0 }) else { throw NativeError.invalid("invalid scroll target") }
            scroll.scroll(to: CGPoint(x: offset[0], y: offset[1])); return true
        }
        if let command = snapshot["zoomTo"] as? [String: Any] {
            guard let id = command["node"] as? String, let scroll = views[id] as? JadixScrollView,
                  (descriptors[id]?["style"] as? [String: Any])?["scroll_zoom"] != nil,
                  let scale = command["scale"] as? Double, scale.isFinite, scale > 0,
                  let anchor = command["anchor"] as? [Double], anchor.count == 2, anchor.allSatisfy({ $0.isFinite }) else { throw NativeError.invalid("invalid zoom target") }
            scroll.zoom(to: CGFloat(scale), anchor: CGPoint(x: anchor[0], y: anchor[1])); return true
        }
        return false
    }
    private func bindHosted(_ hosted: JadixHostedView, id: String, node: [String: Any]) {
                let mountedRoot = root
                hosted.remoteImages = imageLoader()
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
                        self.emit(hosted, ["kind":"hosted", "event":event, "data":data])
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
}

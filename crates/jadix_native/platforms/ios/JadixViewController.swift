import UIKit

/// Factories are registered by the embedding application, never resolved from script names.
@MainActor public final class JadixWindowContentFactories {
    public struct Content: Codable {
        public let factory: String
        public let argument: String?
    }
    public typealias Factory = (Content) throws -> JadixNativeView.RootFactory
    private var factories: [String: Factory] = [:]
    public init() {}
    public func register(_ name: String, factory: @escaping Factory) throws {
        guard !name.isEmpty, name.utf8.count <= 128, factories.count < 64, factories[name] == nil,
              !name.unicodeScalars.contains(where: { CharacterSet.whitespacesAndNewlines.union(.controlCharacters).contains($0) }) else {
            throw JadixControllerError.invalid("Invalid or duplicate content factory")
        }
        factories[name] = factory
    }
    fileprivate func resolve(_ value: [String: Any]) throws -> JadixNativeView.RootFactory {
        let content = try JSONDecoder().decode(Content.self, from: JSONSerialization.data(withJSONObject: value))
        guard let factory = factories[content.factory] else { throw JadixControllerError.invalid("Unregistered content factory: \(content.factory)") }
        return try factory(content)
    }
}

private enum JadixControllerError: LocalizedError {
    case invalid(String)
    var errorDescription: String? { if case let .invalid(message) = self { return message }; return nil }
}

/// Owns one Rust root. Navigation, modal pages and embedded applications use the same host.
@MainActor open class JadixViewController: UIViewController, UIAdaptivePresentationControllerDelegate {
    public let native = JadixNativeView()
    public var rootFactory: JadixNativeView.RootFactory?
    public let contentFactories: JadixWindowContentFactories
    public var prefersLargeTitle = false {
        didSet { if isViewLoaded { configureNavigation() } }
    }
    public var extendsUnderNavigationBar = false {
        didSet { if isViewLoaded { configureNavigation(); view.setNeedsLayout() } }
    }
    public var onMountError: ((Error) -> Void)?
    private var registry: JadixPageRegistry?
    fileprivate weak var sharedRegistry: JadixPageRegistry?
    fileprivate weak var owningPage: JadixViewController?
    fileprivate weak var presentationParent: JadixViewController?
    fileprivate var pageID = "primary"
    fileprivate var attributes: [String: Any] = [:]
    private var mounted = false
    private var closed = false
    fileprivate var closeNotified = false
    private var statePending = false
    private var appearing = false
    private weak var lastWindow: UIWindow?
    private var deepLinks: [(URL, String)] = []
    private var notifications: [NSObjectProtocol] = []

    public init(factory: JadixNativeView.RootFactory? = nil, contentFactories: JadixWindowContentFactories = JadixWindowContentFactories()) {
        self.rootFactory = factory; self.contentFactories = contentFactories
        super.init(nibName: nil, bundle: nil)
        native.handoffEnabled = JadixNativeView.declaresHandoff; native.handoffActive = false
    }
    public required init?(coder: NSCoder) {
        contentFactories = JadixWindowContentFactories(); super.init(coder: coder)
        native.handoffEnabled = JadixNativeView.declaresHandoff; native.handoffActive = false
    }
    open override func viewDidLoad() {
        super.viewDidLoad(); view.backgroundColor = .systemBackground
        native.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(native)
        NSLayoutConstraint.activate([
            native.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor),
            native.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor),
            native.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            native.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor)
        ])
        if sharedRegistry == nil {
            let pages = JadixPageRegistry(primary: self); registry = pages; sharedRegistry = pages
        }
        native.onWindowRequest = { [weak self] id, request in
            guard let self, let pages = self.sharedRegistry else { return false }
            pages.handle(owner: self, id: id, request: request); return true
        }
        for name in [UIApplication.didBecomeActiveNotification, UIApplication.willResignActiveNotification,
                     UIApplication.didEnterBackgroundNotification, UIScreen.modeDidChangeNotification] {
            notifications.append(NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                DispatchQueue.main.async { self?.publishWindowState(); self?.native.invalidateEnvironment() }
            })
        }
        configureNavigation()
    }
    fileprivate func configureNavigation() {
        edgesForExtendedLayout = extendsUnderNavigationBar ? [.top, .bottom] : []
        navigationItem.largeTitleDisplayMode = prefersLargeTitle ? .always : .never
        navigationController?.navigationBar.prefersLargeTitles = prefersLargeTitle
        navigationController?.setNavigationBarHidden(attributes["title_bar"] as? String == "hidden" || attributes["mode"] as? String == "full_screen", animated: false)
        setNeedsStatusBarAppearanceUpdate()
        let allowsClose = attributes["close_on_press"] as? Bool != false
        navigationItem.hidesBackButton = !allowsClose
        if navigationController?.viewControllers.first !== self {
            navigationItem.leftBarButtonItem = allowsClose ? UIBarButtonItem(image: UIImage(systemName: "chevron.backward"), style: .plain, target: self, action: #selector(backPage)) : nil
            navigationItem.leftBarButtonItem?.accessibilityLabel = NSLocalizedString("Back", comment: "Page back action")
        }
        if navigationController?.topViewController === self { navigationController?.interactivePopGestureRecognizer?.isEnabled = allowsClose }

    }
    open override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated); configureNavigation(); updateOrientations()
    }
    open override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        appearing = true; lastWindow = view.window; native.handoffActive = true
        guard !closed else { return }
        if !mounted {
            do {
                try native.mount(factory: rootFactory); mounted = true
                var schemes = (Bundle.main.object(forInfoDictionaryKey: "CFBundleURLTypes") as? [[String: Any]] ?? [])
                    .flatMap { $0["CFBundleURLSchemes"] as? [String] ?? [] }
                if Bundle.main.object(forInfoDictionaryKey: "NSUserActivityTypes") != nil { schemes += ["http", "https"] }
                try native.configureDeepLinks(schemes: schemes)
                didMount()
                let pending = deepLinks; deepLinks.removeAll()
                for (url, source) in pending { try native.deliverDeepLink(url, source: source) }
            } catch { onMountError?(error); native.onError?(error) }
        }
        presentationController?.delegate = self; publishWindowState()
    }
    /// Called once after a successful mount, including roots created by content factories.
    open func didMount() {}
    open override func viewDidLayoutSubviews() { super.viewDidLayoutSubviews(); publishWindowState() }
    open override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated); appearing = false; native.handoffActive = false; publishWindowState()
        if isMovingFromParent || isBeingDismissed || navigationController?.isBeingDismissed == true {
            if !view.isHidden, !closed {
                native.reportWindowEvent(["event": "close_press", "reason": "close_button"])
                sharedRegistry?.removed(self, cause: "close_press")
            }
        } else if attributes["close_on_blur"] as? Bool == true, !view.isHidden,
                  sharedRegistry?.hasVisibleChild(self) != true {
            sharedRegistry?.removed(self, cause: "blur"); sharedRegistry?.hide(self)
        }
    }
    public func openURL(_ url: URL, source: String = "hot_start") throws {
        guard !closed else { throw JadixControllerError.invalid("Closed page") }
        if mounted { try native.deliverDeepLink(url, source: source) }
        else {
            guard deepLinks.count < 64, url.absoluteString.utf8.count <= 16384 else { throw JadixControllerError.invalid("Deep link queue is full") }
            deepLinks.append((url, source))
        }
    }
    /// Forward UIApplication/UIScene activity continuation here, including cold launch.
    @discardableResult public func continueUserActivity(_ activity: NSUserActivity) -> Bool {
        guard !closed, native.handoffEnabled, let url = JadixHandoff.continuedURL(activity) else { return false }
        do { try openURL(url, source: "handoff"); return true }
        catch { native.onError?(error); return false }
    }
    open override var supportedInterfaceOrientations: UIInterfaceOrientationMask {
        switch attributes["screen_orientations"] as? String {
        case "landscape": return .landscape
        case "portrait": return .portrait
        default: return .allButUpsideDown
        }
    }
    open override var shouldAutorotate: Bool { true }
    open override var prefersStatusBarHidden: Bool { attributes["mode"] as? String == "full_screen" }
    open override var keyCommands: [UIKeyCommand]? {
        [UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(escapePage))]
    }
    @objc private func backPage() {
        guard attributes["close_on_press"] as? Bool != false else { return }
        native.reportWindowEvent(["event": "close_press", "reason": "close_button"])
        sharedRegistry?.removed(self, cause: "close_press"); sharedRegistry?.hide(self)
    }
    @objc private func escapePage() { _ = accessibilityPerformEscape() }
    open override func accessibilityPerformEscape() -> Bool {
        guard !closed, !view.isHidden, presentedViewController == nil,
              attributes["close_on_escape"] as? Bool != false, !native.hasMarkedText else { return false }
        native.reportWindowEvent(["event": "close_press", "reason": "shortcut"])
        sharedRegistry?.removed(self, cause: "escape"); sharedRegistry?.hide(self)
        return true
    }
    fileprivate func updateOrientations() {
        if #available(iOS 16, *) {
            setNeedsUpdateOfSupportedInterfaceOrientations()
            navigationController?.setNeedsUpdateOfSupportedInterfaceOrientations()
            view.window?.windowScene?.requestGeometryUpdate(.iOS(interfaceOrientations: supportedInterfaceOrientations)) { [weak self] error in self?.native.onError?(error) }
        } else { UIViewController.attemptRotationToDeviceOrientation() }
    }
    fileprivate func apply(_ update: [String: Any]) throws {
        if let orientation = update["screen_orientations"] as? String,
           !["landscape", "portrait", "landscape_and_portrait"].contains(orientation) { throw JadixControllerError.invalid("Invalid orientation policy") }
        for (key, value) in update { attributes[key] = value }
        if let title = attributes["title"] as? String { self.title = title }
        switch attributes["appearance"] as? String { case "light": overrideUserInterfaceStyle = .light; case "dark": overrideUserInterfaceStyle = .dark; default: overrideUserInterfaceStyle = .unspecified }
        if let size = attributes["content_size"] as? [String: Any], let width = size["width"] as? Double, let height = size["height"] as? Double {
            preferredContentSize = CGSize(width: width, height: height)
        }
        isModalInPresentation = attributes["close_on_press"] as? Bool == false
        configureNavigation()
        updateOrientations(); native.invalidateEnvironment(); publishWindowState()
    }
    fileprivate func publishWindowState() {
        guard mounted, !closed, !statePending else { return }; statePending = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }; self.statePending = false
            guard self.mounted, !self.closed, let window = self.view.window ?? self.lastWindow else { return }
            let frame = self.view.convert(self.view.bounds, to: window.screen.coordinateSpace)
            func rect(_ value: CGRect) -> [String: Double] { ["x": value.minX, "y": value.minY, "width": value.width, "height": value.height] }
            let visible = self.appearing && !self.view.isHidden && window.windowScene?.activationState != .background
            let active = visible && (self.navigationController?.visibleViewController ?? self) === self && self.presentedViewController == nil
            self.native.handoffActive = active
            let state: [String: Any] = ["id": self.pageID, "frame": rect(frame), "normal_frame": rect(frame),
                "content_width": self.native.bounds.width, "content_height": self.native.bounds.height, "backing_scale": window.screen.scale,
                "display": ["frame": rect(window.screen.bounds), "work_area": rect(window.bounds.inset(by: window.safeAreaInsets))],
                "title_bar_insets": ["top": self.view.safeAreaInsets.top, "left": self.view.safeAreaInsets.left, "right": self.view.safeAreaInsets.right],
                "mode": self.attributes["mode"] as? String ?? "normal", "key": active && window.isKeyWindow,
                "visible": visible, "occluded": !active]
            self.native.reportWindowState(state)
            self.owningPage?.native.reportWindowEvent(["event": "child_state", "id": self.pageID, "state": state])
        }
    }
    public func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard !view.isHidden, !closed else { return }
        native.reportWindowEvent(["event": "close_press", "reason": "close_button"])
        sharedRegistry?.removed(self, cause: "close_press")
    }
    public func presentationControllerDidAttemptToDismiss(_ presentationController: UIPresentationController) {
        native.reportWindowEvent(["event": "close_press", "reason": "close_button"])
    }
    fileprivate func dispose(cause: String) {
        guard !closed else { return }; closed = true; deepLinks.removeAll()
        if !closeNotified { closeNotified = true; native.reportWindowEvent(["event": "closed", "cause": cause]) }
        native.close()
        owningPage?.native.reportWindowEvent(["event": "child_state", "id": pageID, "state": NSNull()])
        owningPage?.native.reportWindowEvent(["event": "child_destroyed", "id": pageID])
        presentationParent?.updateOrientations()
    }
    deinit { notifications.forEach(NotificationCenter.default.removeObserver) }
}

/// UINavigationController otherwise ignores the orientation policy of its visible page.
@MainActor public final class JadixNavigationController: UINavigationController {
    public override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        guard !isNavigationBarHidden, navigationBar.prefersLargeTitles,
              topViewController?.navigationItem.largeTitleDisplayMode == .always else { return }
        func firstScroll(_ view: UIView) -> UIScrollView? {
            if let scroll = view as? UIScrollView { return scroll }
            for child in view.subviews { if let scroll = firstScroll(child) { return scroll } }
            return nil
        }
        if let page = topViewController?.view, let scroll = firstScroll(page),
           scroll.contentOffset.y > -scroll.adjustedContentInset.top { return }
        // Preserve an intentionally collapsed title after scrolling; fit only the initial expanded state.
        if navigationBar.sizeThatFits(navigationBar.frame.size).height > navigationBar.frame.height { navigationBar.sizeToFit() }
    }

    public override var supportedInterfaceOrientations: UIInterfaceOrientationMask { visibleViewController?.supportedInterfaceOrientations ?? .allButUpsideDown }
    public override var shouldAutorotate: Bool { visibleViewController?.shouldAutorotate ?? true }
    public override var childForStatusBarHidden: UIViewController? { visibleViewController }
}

@MainActor fileprivate final class JadixPageRegistry {
    weak var primary: JadixViewController?
    private var pages: [String: JadixViewController] = [:]
    init(primary: JadixViewController) { self.primary = primary }
    private func page(_ id: String, owner: JadixViewController) throws -> JadixViewController {
        let target = id == "primary" ? primary : pages[id]
        guard let target, target === owner || target.owningPage === owner else { throw JadixControllerError.invalid("Unknown page or page owned by another root") }
        return target
    }
    func handle(owner: JadixViewController, id: UInt64, request: [String: Any]) {
        do {
            guard let operation = request["operation"] as? String, let name = request["id"] as? String else { throw JadixControllerError.invalid("Invalid window request") }
            var closing: JadixViewController?
            switch operation {
            case "open", "open_declared":
                if let existing = pages[name] { _ = try page(name, owner: owner); try show(existing, owner: owner) }
                else {
                    guard name != "primary", !name.isEmpty, name.utf8.count <= 128, pages.count < 64 else { throw JadixControllerError.invalid("Invalid page ID or page limit exceeded") }
                    let parent = try (request["parent"] as? String).map { try page($0, owner: owner) } ?? owner
                    guard let content = request["content"] as? [String: Any] else { throw JadixControllerError.invalid("An independent page requires a registered content factory") }
                    let factory = try owner.contentFactories.resolve(content)
                    let controller = JadixViewController(factory: factory, contentFactories: owner.contentFactories)
                    controller.pageID = name; controller.owningPage = owner; controller.presentationParent = parent; controller.sharedRegistry = self
                    var attributes = request["attributes"] as? [String: Any] ?? [:]
                    for key in ["title", "resizable", "close_on_blur"] where attributes[key] == nil { attributes[key] = request[key] }
                    if attributes["content_size"] == nil, let width = request["width"], let height = request["height"] { attributes["content_size"] = ["width": width, "height": height] }
                    if attributes["mode"] == nil { attributes["mode"] = request["initial_mode"] ?? "normal" }
                    controller.attributes["kind"] = request["kind"] as? String ?? "normal"
                    controller.loadViewIfNeeded(); try controller.apply(attributes)
                    let kind = request["kind"] as? String ?? "normal"
                    controller.modalPresentationStyle = kind == "overlay" || kind == "popup" ? .overFullScreen : kind == "normal" ? .fullScreen : .formSheet
                    pages[name] = controller
                    controller.view.isHidden = true
                    do {
                        if request["initially_visible"] as? Bool != false && attributes["visible"] as? Bool != false {
                            try setMode(attributes["mode"] as? String ?? "normal", page: controller, owner: parent)
                        }
                    } catch { pages.removeValue(forKey: name); controller.dispose(cause: "close"); throw error }
                }
            case "update", "update_declared":
                let target = try page(name, owner: owner), attributes = request["attributes"] as? [String: Any] ?? [:]
                try target.apply(attributes)
                if let visible = attributes["visible"] as? Bool { if visible { try show(target, owner: owner) } else { hide(target) } }
                if let mode = attributes["mode"] as? String { try setMode(mode, page: target, owner: owner) }
            case "focus": try show(page(name, owner: owner), owner: owner)
            case "restore": try setMode("normal", page: page(name, owner: owner), owner: owner)
            case "set_mode": try setMode(request["mode"] as? String ?? "normal", page: page(name, owner: owner), owner: owner)
            case "set_visibility":
                let target = try page(name, owner: owner)
                if request["visible"] as? Bool == true { try show(target, owner: owner) } else { hide(target) }
            case "close", "close_declared": closing = try page(name, owner: owner)
            default: throw JadixControllerError.invalid("Unsupported page operation")
            }
            owner.native.completeRequest("window", id: id, result: ["Ok": NSNull()])
            // The reply must reach Rust before destroying the requesting root.
            if let closing { DispatchQueue.main.async { [weak self] in self?.removed(closing, cause: "close"); self?.hide(closing) } }
        } catch { owner.native.completeRequest("window", id: id, result: ["Err": error.localizedDescription]) }
    }
    private func show(_ page: JadixViewController, owner: JadixViewController) throws {
        page.loadViewIfNeeded(); page.closeNotified = false
        let owner = page.presentationParent ?? owner
        if page === primary { page.view.isHidden = false; page.view.window?.makeKeyAndVisible(); page.publishWindowState(); return }
        if let navigation = page.navigationController {
            page.view.isHidden = false
            navigation.popToViewController(page, animated: true); page.publishWindowState(); return
        }
        if page.presentingViewController != nil { page.view.isHidden = false; page.publishWindowState(); return }
        if page.attributes["kind"] as? String == "normal", let navigation = owner.navigationController {
            page.view.isHidden = false
            navigation.pushViewController(page, animated: true)
        } else {
            guard owner.view.window != nil, owner.presentedViewController == nil else { throw JadixControllerError.invalid("The parent page is not available for presentation") }
            page.view.isHidden = false
            owner.present(page, animated: true) { page.presentationController?.delegate = page }
        }
    }
    func hasVisibleChild(_ page: JadixViewController) -> Bool {
        pages.values.contains { ($0.presentationParent === page || $0.owningPage === page) && !$0.view.isHidden }
    }
    fileprivate func hide(_ page: JadixViewController) {
        for child in pages.values.filter({ $0.presentationParent === page || $0.owningPage === page }) { hide(child) }
        page.view.isHidden = true
        if let navigation = page.navigationController, page !== primary {
            // Programmatic hiding retains the Rust root for a subsequent restore.
            navigation.setViewControllers(navigation.viewControllers.filter { $0 !== page }, animated: true)
        } else if page.presentingViewController != nil { page.dismiss(animated: true) }
        page.publishWindowState()
    }
    private func setMode(_ mode: String, page: JadixViewController, owner: JadixViewController) throws {
        guard ["normal", "full_screen", "minimized"].contains(mode) else { throw JadixControllerError.invalid("iOS pages do not support desktop maximization") }
        page.attributes["mode"] = mode
        page.configureNavigation()
        if mode == "minimized" { hide(page); return }
        let kind = page.attributes["kind"] as? String ?? "normal"
        if page !== primary, page.navigationController == nil, kind != "overlay", kind != "popup" {
            let presentation: UIModalPresentationStyle = mode == "full_screen" || kind == "normal" ? .fullScreen : .formSheet
            if page.presentingViewController != nil, page.modalPresentationStyle != presentation {
                page.view.isHidden = true
                page.dismiss(animated: false) { [weak self, weak page, weak owner] in
                    guard let self, let page, let owner else { return }
                    page.modalPresentationStyle = presentation
                    do { try self.show(page, owner: owner) } catch { owner.native.onError?(error) }
                }; return
            }
            page.modalPresentationStyle = presentation
        }
        try show(page, owner: owner)
    }
    func removed(_ page: JadixViewController, cause: String) {
        // A hidden page is still registered; actual close/navigation teardown is explicit.
        if page.view.isHidden && cause == "close_press" { return }
        if cause != "close", cause != "parent_close", page.attributes["dismiss_on_close"] as? Bool == true {
            if !page.closeNotified { page.closeNotified = true; page.native.reportWindowEvent(["event": "closed", "cause": cause]) }
            hide(page); return
        }
        let children = pages.values.filter { $0.presentationParent === page || $0.owningPage === page }
        for child in children { removed(child, cause: "parent_close"); hide(child) }
        pages.removeValue(forKey: page.pageID); page.dispose(cause: cause)
    }
}

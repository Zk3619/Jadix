import UIKit

class NativeController: UIViewController {
    let native = JadixNativeView()
    private var keyboard = CGRect.null
    private var observations: [NSObjectProtocol] = []
    private var mounted = false

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        native.onError = { error in NSLog("Jadix native: %@", String(describing: error)) }
        view.addSubview(native)
        observations.append(NotificationCenter.default.addObserver(forName: UIResponder.keyboardWillChangeFrameNotification, object: nil, queue: .main) { [weak self] event in
            self?.keyboard = event.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect ?? .null
            self?.view.setNeedsLayout()
        })
        for notification in [UIResponder.keyboardWillHideNotification, UIApplication.didEnterBackgroundNotification] {
            observations.append(NotificationCenter.default.addObserver(forName: notification, object: nil, queue: .main) { [weak self] _ in
                self?.keyboard = .null
                self?.view.setNeedsLayout()
            })
        }
        observations.append(NotificationCenter.default.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
            self?.view.setNeedsLayout()
            self?.native.invalidateEnvironment()
        })
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        var frame = view.bounds.inset(by: view.safeAreaInsets)
        if let window = view.window, !keyboard.isNull {
            let overlap = frame.intersection(view.convert(window.convert(keyboard, from: nil), from: window))
            if !overlap.isNull && overlap.maxY >= frame.maxY { frame.size.height = max(0, frame.height - overlap.height) }
        }
        native.frame = frame
        native.setNeedsLayout()
        native.layoutIfNeeded()
    }
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        if !mounted {
            do {
                try native.mount(); mounted = true; didMount()
                #if JADIX_NATIVE_CHECKS
                #if JADIX_POPOVER
                DispatchQueue.main.async { NativePopoverChecks.run(self) }
                #elseif JADIX_SELECT
                DispatchQueue.main.async { NativeSelectChecks.run(self) }
                #else
                DispatchQueue.main.async { NativeChecks.run(self) }
                #endif
                #endif
            } catch {
                NSLog("Jadix mount: %@", String(describing: error))
                #if JADIX_NATIVE_CHECKS
                #if JADIX_POPOVER
                NativePopoverChecks.mountFailed(error)
                #elseif JADIX_SELECT
                NativeSelectChecks.mountFailed(error)
                #else
                NativeChecks.mountFailed(error)
                #endif
                #endif
            }
        }
    }
    func didMount() {}
    deinit {
        observations.forEach(NotificationCenter.default.removeObserver)
    }
}

final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ application: UIApplication, didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        let window = UIWindow(frame: UIScreen.main.bounds)
        #if JADIX_NOTES
        window.rootViewController = NativeNotesController()
        #else
        window.rootViewController = NativeController()
        #endif
        window.makeKeyAndVisible()
        self.window = window
        return true
    }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))

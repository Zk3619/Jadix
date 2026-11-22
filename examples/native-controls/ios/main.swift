import UIKit

class NativeController: JadixViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        native.onError = { error in NSLog("Jadix native: %@", String(describing: error)) }
        onMountError = { error in
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
    override func didMount() {
        #if JADIX_NATIVE_CHECKS
        #if JADIX_POPOVER
        DispatchQueue.main.async { NativePopoverChecks.run(self) }
        #elseif JADIX_SELECT
        DispatchQueue.main.async { NativeSelectChecks.run(self) }
        #else
        DispatchQueue.main.async { NativeChecks.run(self) }
        #endif
        #endif
    }
}

final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ application: UIApplication, didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        let window = UIWindow(frame: UIScreen.main.bounds)
        #if JADIX_NOTES
        window.rootViewController = JadixNavigationController(rootViewController: NativeNotesController())
        #else
        window.rootViewController = JadixNavigationController(rootViewController: NativeController())
        #endif
        window.makeKeyAndVisible()
        self.window = window
        if let url = options?[.url] as? URL { try? root?.openURL(url, source: "cold_start") }
        return true
    }
    private var root: JadixViewController? {
        (window?.rootViewController as? UINavigationController)?.visibleViewController as? JadixViewController
    }
    func application(_ app: UIApplication, open url: URL, options: [UIApplication.OpenURLOptionsKey: Any] = [:]) -> Bool {
        do { guard let root else { return false }; try root.openURL(url); return true } catch { return false }
    }
    func application(_ application: UIApplication, continue userActivity: NSUserActivity, restorationHandler: @escaping ([UIUserActivityRestoring]?) -> Void) -> Bool {
        guard let url = userActivity.webpageURL ?? (userActivity.userInfo?["url"] as? String).flatMap(URL.init(string:)) else { return false }
        do { guard let root else { return false }; try root.openURL(url, source: "handoff"); return true } catch { return false }
    }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))

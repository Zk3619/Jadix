import AuthenticationServices
import UserNotifications
#if os(iOS)
import UIKit
#else
import AppKit
import Carbon
#endif

/// Install before application launch so notification responses can wait for a root.
@MainActor public func JadixPrepareDesktopServices() { _ = JadixNotificationHub.shared }

#if !os(iOS)
@MainActor private enum JadixDockBadges {
    static var order: [UUID] = []
    static var labels: [UUID: String] = [:]
    static func set(_ owner: UUID, label: String?) {
        order.removeAll { $0 == owner }; order.append(owner); labels[owner] = label ?? ""
        NSApp.dockTile.badgeLabel = label
    }
    static func remove(_ owner: UUID) {
        let visible = order.last == owner
        order.removeAll { $0 == owner }; labels.removeValue(forKey: owner)
        if visible { NSApp.dockTile.badgeLabel = order.last.flatMap { labels[$0] } }
    }
}

#endif

@MainActor private final class JadixNotificationHub: NSObject, UNUserNotificationCenterDelegate {
    static let shared = JadixNotificationHub()
    private struct Owner { let token: UUID; let receive: (String, String, Bool) -> Void }
    private var owners: [String: Owner] = [:]
    private var waiting: [(String, String, String, Bool)] = []
    private weak var previousDelegate: UNUserNotificationCenterDelegate?
    override init() {
        super.init()
        let center = UNUserNotificationCenter.current()
        previousDelegate = center.delegate; center.delegate = self
        center.getNotificationCategories { existing in
            let category = UNNotificationCategory(identifier: "jadix.notification", actions: [], intentIdentifiers: [], options: [.customDismissAction])
            var categories = existing.filter { $0.identifier != category.identifier }; categories.insert(category)
            center.setNotificationCategories(categories)
        }
    }
    func attach(_ scope: String, token: UUID, receive: @escaping (String, String, Bool) -> Void) -> Bool {
        guard owners[scope] == nil else { return owners[scope]?.token == token }
        owners[scope] = Owner(token: token, receive: receive)
        let pending = waiting.filter { $0.0 == scope }; waiting.removeAll { $0.0 == scope }
        // Deliver after the caller has installed its root callback and released publication.
        DispatchQueue.main.async { [weak self] in
            guard self?.owners[scope]?.token == token else { return }
            for item in pending { receive(item.1, item.2, item.3) }
        }
        return true
    }
    func detach(_ scope: String, token: UUID) { if owners[scope]?.token == token { owners.removeValue(forKey: scope) } }
    private func receive(_ scope: String, identifier: String, data: String, dismissed: Bool) {
        guard scope.utf8.count <= 64, identifier.utf8.count <= 256, data.utf8.count <= 4096 else { return }
        if let owner = owners[scope] { owner.receive(identifier, data, dismissed) }
        else { if waiting.count >= 64 { waiting.removeFirst() }; waiting.append((scope, identifier, data, dismissed)) }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
                                            withCompletionHandler completionHandler: @escaping () -> Void) {
        let content = response.notification.request.content
        let scope = content.userInfo["jadix_scope"] as? String
        let data = content.userInfo["jadix_data"] as? String ?? ""
        let identifier = response.notification.request.identifier
        let dismissed = response.actionIdentifier == UNNotificationDismissActionIdentifier
        DispatchQueue.main.async { [weak self] in
            if let scope { self?.receive(scope, identifier: identifier, data: data, dismissed: dismissed); completionHandler() }
            else if let delegate = self?.previousDelegate,
                    delegate.responds(to: #selector(UNUserNotificationCenterDelegate.userNotificationCenter(_:didReceive:withCompletionHandler:))) {
                delegate.userNotificationCenter?(center, didReceive: response, withCompletionHandler: completionHandler)
            } else { completionHandler() }
        }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification,
                                            withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        if notification.request.content.userInfo["jadix_scope"] != nil { completionHandler([.banner, .list, .sound]); return }
        DispatchQueue.main.async { [weak self] in
            if let delegate = self?.previousDelegate,
               delegate.responds(to: #selector(UNUserNotificationCenterDelegate.userNotificationCenter(_:willPresent:withCompletionHandler:))) {
                delegate.userNotificationCenter?(center, willPresent: notification, withCompletionHandler: completionHandler)
            } else { completionHandler([]) }
        }
    }
}

#if !os(iOS)
@MainActor private final class JadixGlobalHotKeys {
    static let shared = JadixGlobalHotKeys()
    private var serial: UInt32 = 0
    private var handler: EventHandlerRef?
    private var registrations: [UInt32: (EventHotKeyRef, () -> Void)] = [:]
    private init() {
        var type = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let context = Unmanaged.passUnretained(self).toOpaque()
        InstallEventHandler(GetApplicationEventTarget(), { _, event, context in
            guard let event, let context else { return OSStatus(eventNotHandledErr) }
            var key = EventHotKeyID()
            let status = GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                                           nil, UInt32(MemoryLayout<EventHotKeyID>.size), nil, &key)
            guard status == noErr, key.signature == 0x4a445849 else { return OSStatus(eventNotHandledErr) }
            let manager = Unmanaged<JadixGlobalHotKeys>.fromOpaque(context).takeUnretainedValue()
            let id = key.id
            DispatchQueue.main.async { manager.registrations[id]?.1() }
            return noErr
        }, 1, &type, context, &handler)
    }
    func register(_ value: [String: Any], action: @escaping () -> Void) -> (UInt32?, String?) {
        guard handler != nil, registrations.count < 256, serial < UInt32.max,
              let key = value["key"] as? String, let code = Self.keys[key] else { return (nil, "unavailable") }
        var modifiers: UInt32 = 0
        for (name, flag) in [("command", cmdKey), ("control", controlKey), ("option", optionKey), ("shift", shiftKey)] {
            if value[name] as? Bool == true { modifiers |= UInt32(flag) }
        }
        serial += 1
        var reference: EventHotKeyRef?
        let status = RegisterEventHotKey(code, modifiers, EventHotKeyID(signature: 0x4a445849, id: serial),
                                         GetApplicationEventTarget(), 0, &reference)
        guard status == noErr, let reference else { return (nil, status == eventHotKeyExistsErr ? "conflict" : "failed") }
        registrations[serial] = (reference, action); return (serial, nil)
    }
    func remove(_ id: UInt32) { if let entry = registrations.removeValue(forKey: id) { UnregisterEventHotKey(entry.0) } }
    private static let keys: [String: UInt32] = [
        "A": 0, "S": 1, "D": 2, "F": 3, "H": 4, "G": 5, "Z": 6, "X": 7, "C": 8, "V": 9,
        "B": 11, "Q": 12, "W": 13, "E": 14, "R": 15, "Y": 16, "T": 17,
        "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23, "9": 25, "7": 26, "8": 28, "0": 29,
        "O": 31, "U": 32, "I": 34, "P": 35, "Enter": 36, "L": 37, "J": 38, "K": 40,
        "N": 45, "M": 46, "Tab": 48, "Space": 49, "Backspace": 51, "Escape": 53,
        "F1": 122, "F2": 120, "F3": 99, "F4": 118, "F5": 96, "F6": 97, "F7": 98, "F8": 100,
        "F9": 101, "F10": 109, "F11": 103, "F12": 111, "F13": 105, "F14": 107, "F15": 113,
        "F16": 106, "F17": 64, "F18": 79, "F19": 80, "F20": 90,
        "Home": 115, "End": 119, "PageUp": 116, "PageDown": 121, "Delete": 117,
        "ArrowLeft": 123, "ArrowRight": 124, "ArrowDown": 125, "ArrowUp": 126]
}

#endif

@MainActor private final class JadixAuthPresentation: NSObject, ASWebAuthenticationPresentationContextProviding {
    let anchor: ASPresentationAnchor
    init(_ anchor: ASPresentationAnchor) { self.anchor = anchor }
    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor { anchor }
}

@MainActor final class JadixDesktopServices {
    var authAnchor: (() -> ASPresentationAnchor?)?
    private var auth: (id: UInt32, session: ASWebAuthenticationSession, presentation: JadixAuthPresentation)?
    private func authReply(_ id: UInt32, outcome: [String: Any]? = nil, error: String? = nil) {
        output?(id, ["kind": "reply", "value": ["type": "auth_session",
            "outcome": outcome.map { $0 as Any } ?? NSNull(), "error": error.map { $0 as Any } ?? NSNull()]])
    }
    private func cancelAuth() {
        guard let running = auth else { return }
        auth = nil; running.session.cancel()
        authReply(running.id, outcome: ["status": "cancelled"])
    }
    private func openAuth(_ id: UInt32, request: [String: Any]) {
        guard let text = request["url"] as? String, text.utf8.count <= 16 * 1024,
              let url = URL(string: text), ["http", "https"].contains(url.scheme?.lowercased() ?? ""),
              url.host != nil, url.user == nil, url.password == nil,
              let scheme = request["callback_scheme"] as? String, scheme.utf8.count <= 64,
              scheme.range(of: "^[A-Za-z][A-Za-z0-9+.-]*$", options: .regularExpression) != nil,
              !["http", "https", "file", "javascript", "vbscript", "data", "blob", "about", "asset"].contains(scheme.lowercased())
        else { authReply(id, error: "failed"); return }
        guard let anchor = authAnchor?() else { authReply(id, error: "unavailable"); return }
        cancelAuth()
        let current = generation, presentation = JadixAuthPresentation(anchor)
        let session = ASWebAuthenticationSession(url: url, callbackURLScheme: scheme) { [weak self] callback, error in
            DispatchQueue.main.async {
                guard let self, self.generation == current, self.auth?.id == id else { return }
                self.auth = nil
                if let callback, callback.scheme?.lowercased() == scheme.lowercased(), callback.absoluteString.utf8.count <= 16 * 1024 {
                    self.authReply(id, outcome: ["status": "completed", "url": callback.absoluteString])
                } else if let error = error as NSError?, error.domain == ASWebAuthenticationSessionError.errorDomain,
                          error.code == ASWebAuthenticationSessionError.canceledLogin.rawValue {
                    self.authReply(id, outcome: ["status": "cancelled"])
                } else { self.authReply(id, error: "failed") }
            }
        }
        session.prefersEphemeralWebBrowserSession = request["ephemeral"] as? Bool ?? false
        session.presentationContextProvider = presentation
        auth = (id, session, presentation)
        if !session.start(), auth?.id == id { auth = nil; authReply(id, error: "failed") }
    }
    var output: ((UInt32, [String: Any]) -> Void)?
    var scope = "main"
    private let token = UUID()
    private var generation = UUID()
    private var attached = false
    private var notifications: [UInt32: String] = [:]
    private var shortcuts: [UInt32: UInt32] = [:]
    private var pending: Set<UInt32> = []
    private var ownsBadge = false
    func start() { _ = attach() }
    private func complete(_ id: UInt32, error: String? = nil) {
        output?(id, ["kind": "reply", "value": ["type": "complete", "error": error.map { $0 as Any } ?? NSNull()]])
    }
    private func event(_ id: UInt32, _ type: String) { output?(id, ["kind": "event", "value": ["type": type]]) }
    private func attach() -> Bool {
        if attached { return true }
        guard !scope.isEmpty, scope.utf8.count <= 64, scope.utf8.allSatisfy({ (48...57).contains($0)
            || (65...90).contains($0) || (97...122).contains($0) || [45, 46, 95].contains($0) }) else { return false }
        attached = JadixNotificationHub.shared.attach(scope, token: token) { [weak self] identifier, data, dismissed in
            guard let self else { return }
            if let id = self.notifications.first(where: { $0.value == identifier })?.key { self.event(id, dismissed ? "dismiss" : "click") }
            else if !dismissed { self.output?(0, ["kind": "activation", "value": ["identifier": identifier, "data": data]]) }
        }
        return attached
    }
    func handle(_ command: [String: Any]) {
        guard let id = command["id"] as? UInt32, let action = command["action"] as? String else { return }
        let center = UNUserNotificationCenter.current(), current = generation
        switch action {
        case "open_auth_session":
            guard let request = command["request"] as? [String: Any] else { authReply(id, error: "failed"); return }
            openAuth(id, request: request)
        case "close_auth_session":
            if auth?.id == id { cancelAuth() }
        case "haptic":
#if os(iOS)
            guard UIApplication.shared.applicationState == .active,
                  let feedback = command["feedback"] as? [String: Any] else { complete(id); return }
            switch feedback["type"] as? String {
            case "selection": UISelectionFeedbackGenerator().selectionChanged()
            case "impact":
                let styles: [String: UIImpactFeedbackGenerator.FeedbackStyle] = ["light": .light, "medium": .medium, "heavy": .heavy, "soft": .soft, "rigid": .rigid]
                guard let style = styles[feedback["style"] as? String ?? ""] else { complete(id, error: "failed"); return }
                UIImpactFeedbackGenerator(style: style).impactOccurred()
            case "notification":
                let kinds: [String: UINotificationFeedbackGenerator.FeedbackType] = ["success": .success, "warning": .warning, "error": .error]
                guard let kind = kinds[feedback["kind"] as? String ?? ""] else { complete(id, error: "failed"); return }
                UINotificationFeedbackGenerator().notificationOccurred(kind)
            default: complete(id, error: "failed"); return
            }
#endif
            complete(id)
        case "permission":
            guard attach(), pending.count < 128, pending.insert(id).inserted else {
                output?(id, ["kind": "reply", "value": ["type": "permission", "authorization": "unknown", "error": "unavailable"]]); return
            }
            let finish: (Error?) -> Void = { [weak self] error in
                center.getNotificationSettings { settings in
                    let status: String
                    switch settings.authorizationStatus.rawValue {
                    case 0: status = "not_determined"
                    case 1: status = "denied"
                    case 2: status = "authorized"
                    case 3: status = "provisional"
                    case 4: status = "ephemeral"
                    default: status = "unknown"
                    }
                    DispatchQueue.main.async {
                        guard let self, self.generation == current, self.pending.remove(id) != nil else { return }
                        self.output?(id, ["kind": "reply", "value": ["type": "permission", "authorization": status,
                            "error": error == nil ? NSNull() : "failed" as Any]])
                    }
                }
            }
            if command["request"] as? Bool == true { center.requestAuthorization(options: [.alert, .sound, .badge]) { _, error in finish(error) } }
            else { finish(nil) }
        case "notify":
            guard attach(), notifications.count < 64, notifications[id] == nil,
                  let value = command["notification"] as? [String: Any] else { complete(id, error: "unavailable"); return }
            let identifier = "jadix.\(scope).\(token.uuidString).\(id)"
            let content = UNMutableNotificationContent()
            content.categoryIdentifier = "jadix.notification"
            content.title = value["title"] as? String ?? ""; content.subtitle = value["subtitle"] as? String ?? ""
            content.body = value["body"] as? String ?? ""
            if value["sound"] as? Bool == true { content.sound = .default }
            content.userInfo = ["jadix_scope": scope, "jadix_data": value["data"] as? String ?? ""]
            notifications[id] = identifier
            center.getNotificationSettings { [weak self] settings in
                DispatchQueue.main.async {
                    guard let self, self.generation == current, self.notifications[id] == identifier else { return }
                    guard settings.authorizationStatus == .authorized || settings.authorizationStatus == .provisional else {
                        self.notifications.removeValue(forKey: id); self.complete(id, error: "denied"); return
                    }
                    center.add(UNNotificationRequest(identifier: identifier, content: content, trigger: nil)) { [weak self] error in
                        DispatchQueue.main.async {
                            guard let self, self.generation == current, self.notifications[id] == identifier else {
                                center.removePendingNotificationRequests(withIdentifiers: [identifier])
                                center.removeDeliveredNotifications(withIdentifiers: [identifier]); return
                            }
                            if error != nil { self.notifications.removeValue(forKey: id) }
                            self.complete(id, error: error == nil ? nil : "failed")
                        }
                    }
                }
            }
        case "cancel_notification":
            if let identifier = notifications.removeValue(forKey: id) {
                center.removePendingNotificationRequests(withIdentifiers: [identifier]); center.removeDeliveredNotifications(withIdentifiers: [identifier])
            }
        case "badge":
            guard attach() else { complete(id, error: "unavailable"); return }
            #if os(iOS)
            if let label = command["label"] as? String, let count = Int(label), count >= 0 {
                UIApplication.shared.applicationIconBadgeNumber = count; ownsBadge = true; complete(id)
            } else if command["label"] == nil || command["label"] is NSNull {
                UIApplication.shared.applicationIconBadgeNumber = 0; ownsBadge = true; complete(id)
            } else { complete(id, error: "unavailable") }
#else
            JadixDockBadges.set(token, label: command["label"] as? String); ownsBadge = true; complete(id)
#endif
        case "register_shortcut":
#if os(iOS)
            complete(id, error: "unavailable")
#else
            guard shortcuts.count < 64, shortcuts[id] == nil, let value = command["shortcut"] as? [String: Any] else { complete(id, error: "unavailable"); return }
            let (registration, error) = JadixGlobalHotKeys.shared.register(value) { [weak self] in self?.event(id, "shortcut") }
            if let registration { shortcuts[id] = registration }; complete(id, error: error)
#endif
        case "unregister_shortcut":
#if !os(iOS)
            if let registration = shortcuts.removeValue(forKey: id) { JadixGlobalHotKeys.shared.remove(registration) }
#endif
        default: complete(id, error: "unavailable")
        }
    }
    func dispose() {
        generation = UUID(); cancelAuth(); authAnchor = nil; pending.removeAll()
        let identifiers = Array(notifications.values); notifications.removeAll()
        UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: identifiers)
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: identifiers)
#if !os(iOS)
        for registration in shortcuts.values { JadixGlobalHotKeys.shared.remove(registration) }; shortcuts.removeAll()
        if ownsBadge { JadixDockBadges.remove(token); ownsBadge = false }
#else
        if ownsBadge { UIApplication.shared.applicationIconBadgeNumber = 0; ownsBadge = false }
#endif
        if attached { JadixNotificationHub.shared.detach(scope, token: token); attached = false }
        output = nil
    }
}

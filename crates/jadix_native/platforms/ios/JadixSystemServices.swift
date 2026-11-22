import UIKit
import UniformTypeIdentifiers

@MainActor final class JadixSystemServices: NSObject, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
    weak var host: UIView?
    var alive: () -> Bool = { false }
    var respond: (String, UInt64, [String: Any]) -> Void = { _, _, _ in }
    var lookup: (String) -> UIView? = { _ in nil }
    private var contextMenu: JadixContextMenuPresentation?
    private var settling = false
    private var picker: UIDocumentPickerViewController?
    private var fileID: UInt64?
    private var alert: UIAlertController?
    private var alertID: UInt64?
    private var activity: UIActivityViewController?
    private var shareID: UInt64?
    private var shareReplyKind = "share"
    private var scoped: Set<URL> = []
    private var pending: [(String, UInt64, [String: Any])] = []
    private var drainGeneration = UUID()
    private var exported: URL?
    private var observers: [NSObjectProtocol] = []
    init(host: UIView) { self.host = host; super.init() }
    private var presenter: UIViewController? {
        var next: UIResponder? = host
        while let value = next {
            if let controller = value as? UIViewController {
                var top = controller
                while let presented = top.presentedViewController, !presented.isBeingDismissed { top = presented }
                return top
            }
            next = value.next
        }
        return nil
    }
    func handle(_ kind: String, id: UInt64, request: [String: Any]) {
        guard alive() else { return }
        if kind == "alert", request["operation"] as? String == "close" {
            pending.removeAll { $0.0 == "alert" && $0.1 == id }
            if alertID == id { alert?.dismiss(animated: false); finishAlert(id, button: nil) }
            return
        }
        if kind == "share", let previous = shareID, let controller = activity, !settling {
            guard pending.count < 64 else { respond(kind, id, ["Err": "System presentation queue is full"]); return }
            controller.completionWithItemsHandler = nil
            activity = nil; shareID = nil
            respond(shareReplyKind, previous, shareReplyKind == "file" ? ["Ok": []] : ["Ok": ["status": "replaced"]])
            // Presentation must finish dismissing before a replacement is shown.
            pending.insert((kind, id, request), at: 0); drain(after: controller); return
        }
        if settling || picker != nil || alert != nil || activity != nil || contextMenu != nil {
            guard pending.count < 64 else { respond(kind, id, ["Err": "System presentation queue is full"]); return }
            pending.append((kind, id, request)); return
        }
        switch kind {
        case "file": file(id, request)
        case "alert": showAlert(id, request)
        case "share": share(id, request)
        case "context":
            guard let anchorID = request["anchor"] as? String, let anchor = lookup(anchorID), anchor.window != nil, !anchor.isHidden, anchor.alpha > 0, let presenter else { respond(kind, id, ["Err": "Context menu anchor is not visible"]); return }
            contextMenu = JadixContextMenuPresentation(anchor: anchor, presenter: presenter, items: request["items"] as? [[String: Any]] ?? []) { [weak self] result in
                guard let self, self.alive() else { return }; self.contextMenu = nil; self.respond("context", id, result); self.drain()
            }
            contextMenu?.show()
        default: respond(kind, id, ["Err": "The embedding controller does not support this window operation"])
        }
    }
    private func drain(after controller: UIViewController? = nil) {
        let generation = drainGeneration
        let resume: () -> Void = { [weak self] in
            guard let self, self.alive(), self.drainGeneration == generation else { return }
            self.settling = false
            guard self.picker == nil, self.alert == nil, self.activity == nil, self.contextMenu == nil, !self.pending.isEmpty else { return }
            let request = self.pending.removeFirst(); self.handle(request.0, id: request.1, request: request.2)
            if !self.settling, self.picker == nil, self.alert == nil, self.activity == nil, self.contextMenu == nil { self.drain() }
        }
        settling = true
        if let controller, controller.presentingViewController != nil {
            if let transition = controller.transitionCoordinator, controller.isBeingDismissed {
                transition.animate(alongsideTransition: nil) { _ in resume() }
            } else { controller.dismiss(animated: true, completion: resume) }
        } else { DispatchQueue.main.async(execute: resume) }
    }
    private func file(_ id: UInt64, _ request: [String: Any]) {
        let operation = request["operation"] as? String ?? ""
        if operation == "open_url", let value = request["url"] as? String,
           value.utf8.count <= 16384, let url = URL(string: value), let scheme = url.scheme?.lowercased(),
           !["javascript", "vbscript", "data", "blob", "file", "about", "asset"].contains(scheme),
           !value.unicodeScalars.contains(where: { CharacterSet.whitespacesAndNewlines.union(.controlCharacters).contains($0) }),
           !["http", "https"].contains(scheme) || url.host?.isEmpty == false {
            UIApplication.shared.open(url, options: [:]) { [weak self] ok in
                Task { @MainActor in guard let self, self.alive() else { return }; self.respond("file", id, ok ? ["Ok": []] : ["Err": "The system could not open this URL"]) }
            }; return
        }
        if operation == "open" || operation == "reveal" {
            guard let path = request["path"] as? String, path.hasPrefix("/"), FileManager.default.fileExists(atPath: path) else { respond("file", id, ["Err": "A readable absolute file is required"]); return }
            share(id, ["files": [path]], replyKind: "file"); return
        }
        // UIKit has no desktop trash service. Never silently delete a user's document.
        guard operation == "pick_paths" || operation == "pick_save_path" else { respond("file", id, ["Err": "This file operation is unavailable on iOS"]); return }
        guard picker == nil, alert == nil, activity == nil, let presenter, host?.window != nil else { respond("file", id, ["Err": "A visible, unoccupied presentation controller is required"]); return }
        let options = request["options"] as? [String: Any] ?? [:]
        let identifiers = options["content_types"] as? [String] ?? []
        let types = identifiers.compactMap(UTType.init)
        guard types.count == identifiers.count else { respond("file", id, ["Err": "Unknown document content type"]); return }
        let panel: UIDocumentPickerViewController
        if operation == "pick_save_path" {
            let name = request["file_name"] as? String ?? "Untitled"
            guard !name.isEmpty, name == (name as NSString).lastPathComponent, ![".", ".."].contains(name) else { respond("file", id, ["Err": "A plain file name is required"]); return }
            let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                let source = directory.appendingPathComponent(name); try Data().write(to: source)
                exported = directory; panel = UIDocumentPickerViewController(forExporting: [source], asCopy: true)
            } catch { try? FileManager.default.removeItem(at: directory); respond("file", id, ["Err": error.localizedDescription]); return }
        } else {
            let selection = request["selection"] as? String ?? "files"
            let allowed = selection == "folders" ? [UTType.folder] : (types.isEmpty ? [UTType.item] : types) + (selection == "files_and_folders" ? [.folder] : [])
            panel = UIDocumentPickerViewController(forOpeningContentTypes: allowed, asCopy: false)
            panel.allowsMultipleSelection = request["multiple"] as? Bool ?? false
        }
        panel.title = options["title"] as? String
        if let path = options["directory"] as? String, path.hasPrefix("/") { panel.directoryURL = URL(fileURLWithPath: path) }
        panel.delegate = self; picker = panel; fileID = id
        presenter.present(panel, animated: true); panel.presentationController?.delegate = self
    }
    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        guard alive(), picker === controller, let id = fileID else { return }
        guard urls.count <= 1024 else { finishFile(id, ["Err": "Document selection exceeds quota"]); return }
        let limited = urls
        guard limited.allSatisfy({ scoped.contains($0) }) || scoped.union(limited).count <= 1024 else {
            finishFile(id, ["Err": "Security-scoped document quota exceeded"]); return
        }
        var acquired: [URL] = []
        for url in limited where !scoped.contains(url) {
            if url.startAccessingSecurityScopedResource() { acquired.append(url) }
        }
        guard limited.allSatisfy({ $0.isFileURL && FileManager.default.isReadableFile(atPath: $0.path) }) else {
            for url in acquired { url.stopAccessingSecurityScopedResource() }
            finishFile(id, ["Err": "The selected document is not readable"]); return
        }
        scoped.formUnion(acquired)
        finishFile(id, ["Ok": limited.map(\.path)])
    }
    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) { if picker === controller, let id = fileID { finishFile(id, ["Ok": []]) } }
    private func finishFile(_ id: UInt64, _ result: [String: Any]) {
        let controller = picker; picker = nil; fileID = nil
        if let exported { try? FileManager.default.removeItem(at: exported) }; exported = nil
        respond("file", id, result)
        drain(after: controller)
    }
    private func showAlert(_ id: UInt64, _ request: [String: Any]) {
        if request["operation"] as? String == "close" {
            if alertID == id { alert?.dismiss(animated: true); finishAlert(id, button: nil) }; return
        }
        guard alert == nil, picker == nil, activity == nil, let presenter, let value = request["alert"] as? [String: Any],
              let buttons = value["buttons"] as? [String], !buttons.isEmpty, buttons.count <= 32 else { respond("alert", id, ["Err": "An alert requires a free visible controller and valid buttons"]); return }
        let controller = UIAlertController(title: value["title"] as? String, message: value["message"] as? String, preferredStyle: .alert)
        let input = value["input"] as? [String: Any]
        if let input { controller.addTextField { field in field.text = input["value"] as? String; field.placeholder = input["placeholder"] as? String; field.accessibilityLabel = input["accessible_name"] as? String } }
        for (index, title) in buttons.enumerated() {
            let cancel = index == value["cancel_button"] as? Int
            let action = UIAlertAction(title: title, style: cancel ? .cancel : .default) { [weak self] _ in self?.finishAlert(id, button: index) }
            controller.addAction(action)
            if index == value["default_button"] as? Int { controller.preferredAction = action }
        }
        if input?["required"] as? Bool == true, let field = controller.textFields?.first {
            let update: () -> Void = { [weak controller, weak field] in
                for (index, action) in (controller?.actions ?? []).enumerated() { action.isEnabled = index == value["cancel_button"] as? Int || field?.text?.isEmpty == false }
            }
            observers.append(NotificationCenter.default.addObserver(forName: UITextField.textDidChangeNotification, object: field, queue: .main) { _ in MainActor.assumeIsolated { update() } }); update()
        }
        alert = controller; alertID = id
        presenter.present(controller, animated: true) { [weak controller] in
            if input?["select_text_on_focus"] as? Bool == true { controller?.textFields?.first?.selectAll(nil) }
        }; controller.presentationController?.delegate = self
    }
    private func finishAlert(_ id: UInt64, button: Int?) {
        guard alertID == id else { return }
        let text = alert?.textFields?.first?.text
        let controller = alert; alert = nil; alertID = nil
        for observer in observers { NotificationCenter.default.removeObserver(observer) }; observers.removeAll()
        respond("alert", id, ["Ok": ["button": button.map { $0 as Any } ?? NSNull(), "input": text.map { $0 as Any } ?? NSNull()]])
        drain(after: controller)
    }
    private func share(_ id: UInt64, _ request: [String: Any], replyKind: String = "share") {
        guard picker == nil, alert == nil, let presenter, let host, host.window != nil else { respond(replyKind, id, ["Err": "Sharing requires a visible presentation controller"]); return }
        var items: [Any] = []
        if let text = request["text"] as? String { items.append(text) }
        if let value = request["url"] as? String {
            guard value.utf8.count <= 16384, let url = URL(string: value), let scheme = url.scheme?.lowercased(), !["javascript", "vbscript", "data", "blob", "file", "about", "asset"].contains(scheme), url.user == nil, url.password == nil else { respond(replyKind, id, ["Err": "An absolute non-file share URL is required"]); return }; items.append(url)
        }
        for path in request["files"] as? [String] ?? [] {
            guard path.hasPrefix("/"), FileManager.default.isReadableFile(atPath: path) else { respond(replyKind, id, ["Err": "A readable absolute file is required"]); return }; items.append(URL(fileURLWithPath: path))
        }
        guard !items.isEmpty, items.count <= 1024 else { respond(replyKind, id, ["Err": "Invalid share item count"]); return }
        let controller = UIActivityViewController(activityItems: items, applicationActivities: nil)
        if let subject = request["subject"] as? String { controller.setValue(subject, forKey: "subject") }
        let anchor = (request["anchor"] as? String).flatMap(lookup) ?? host
        guard anchor.window === host.window, !anchor.isHidden, anchor.alpha > 0 else { respond(replyKind, id, ["Err": "Share anchor is not visible"]); return }
        controller.popoverPresentationController?.sourceView = anchor
        controller.popoverPresentationController?.sourceRect = anchor.bounds.isEmpty ? CGRect(x: 0, y: 0, width: 1, height: 1) : anchor.bounds
        controller.completionWithItemsHandler = { [weak self, weak controller] service, completed, _, error in
            guard let self, self.shareID == id, self.alive() else { return }; self.shareID = nil; self.activity = nil
            if let error { self.respond(replyKind, id, ["Err": error.localizedDescription]) }
            else if replyKind == "file" { self.respond("file", id, ["Ok": []]) }
            else { self.respond("share", id, ["Ok": completed ? ["status": "shared", "service": service?.rawValue ?? "system"] : ["status": "dismissed"]]) }
            self.drain(after: controller)
        }
        activity = controller; shareID = id; shareReplyKind = replyKind; presenter.present(controller, animated: true)
        controller.presentationController?.delegate = self
    }
    func presentationControllerDidDismiss(_ controller: UIPresentationController) {
        if controller.presentedViewController === picker, let id = fileID { finishFile(id, ["Ok": []]) }
        if controller.presentedViewController === alert, let id = alertID { finishAlert(id, button: nil) }
        if controller.presentedViewController === activity, let id = shareID {
            activity?.completionWithItemsHandler = nil; activity = nil; shareID = nil
            respond(shareReplyKind, id, shareReplyKind == "file" ? ["Ok": []] : ["Ok": ["status": "dismissed"]]); drain()
        }
    }
    func dispose() {
        drainGeneration = UUID(); pending.removeAll(); settling = false; contextMenu?.dismiss(); contextMenu = nil
        picker?.delegate = nil; picker?.dismiss(animated: false); picker = nil; fileID = nil
        alert?.dismiss(animated: false); alert = nil; alertID = nil
        activity?.completionWithItemsHandler = nil; activity?.dismiss(animated: false); activity = nil; shareID = nil
        for observer in observers { NotificationCenter.default.removeObserver(observer) }; observers.removeAll()
        for url in scoped { url.stopAccessingSecurityScopedResource() }; scoped.removeAll()
        if let exported { try? FileManager.default.removeItem(at: exported) }; exported = nil
    }
}

import UIKit

enum NativePopoverChecks {
    private struct Failure: Error { let message: String }
    private static func require(_ condition: Bool, _ message: String) throws {
        if !condition { throw Failure(message: message) }
    }
    private static func descendants(_ view: UIView) -> [UIView] {
        view.subviews.flatMap { [$0] + descendants($0) }
    }
    private static func button(_ view: UIView, _ title: String) throws -> UIButton {
        guard let button = descendants(view).compactMap({ $0 as? UIButton }).first(where: { $0.title(for: .normal) == title }) else {
            throw Failure(message: "Missing native Button: " + title)
        }
        return button
    }
    private static func input(_ view: UIView, _ placeholder: String) throws -> UITextField {
        guard let input = descendants(view).compactMap({ $0 as? UITextField }).first(where: { $0.placeholder == placeholder }) else {
            throw Failure(message: "Missing native UITextField: " + placeholder)
        }
        return input
    }
    private static func state(_ native: JadixNativeView) throws -> [String: Any] {
        guard let state = try JSONSerialization.jsonObject(with: Data(native.state().utf8)) as? [String: Any] else {
            throw Failure(message: "Invalid Rust state")
        }
        return state
    }
    private static var token: String? {
        guard let value = ProcessInfo.processInfo.environment["JADIX_NATIVE_CHECK_TOKEN"], UUID(uuidString: value) != nil else { return nil }
        return value
    }
    private static func write(_ result: [String: Any]) {
        guard let token = token else { return }
        do {
            let directory = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            var report = result
            report["schema"] = 1
            report["token"] = token
            report["bundle_identifier"] = Bundle.main.bundleIdentifier
            report["system_version"] = UIDevice.current.systemVersion
            report["application"] = "popover"
            report["event_authority"] = "programmatic UIKit controls, text input, scroll and dismissal; not touch, hardware keys, physical IME or VoiceOver"
            let data = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
            try data.write(to: directory.appendingPathComponent("native-checks.json"), options: [.atomic])
        } catch { NSLog("Jadix Popover checks receipt: %@", String(describing: error)) }
    }
    static func mountFailed(_ error: Error) {
        write(["passed": false, "checks": [], "native_errors": [], "error": String(describing: error)])
    }
    private static func geometry(_ native: JadixNativeView, _ panel: NativePopover, _ anchor: UIView) throws -> [String: Any] {
        guard let window = native.window else { throw Failure(message: "Missing attached window") }
        window.layoutIfNeeded()
        let viewport = native.convert(native.bounds, to: window).intersection(window.safeAreaLayoutGuide.layoutFrame)
        let target = anchor.convert(anchor.bounds, to: window)
        let content = panel.content.convert(panel.content.bounds, to: window)
        try require(viewport.contains(content) && content.width > 0 && content.height > 0, "Panel outside visible native viewport")
        try require(abs(content.minX - target.minX) < 1 && abs(content.minY - target.maxY - 4) < 1, "Native anchor or gap mismatch")
        return ["inside_viewport": true, "anchor_left_error": abs(content.minX - target.minX), "anchor_gap_error": abs(content.minY - target.maxY - 4)]
    }
    static func run(_ controller: NativeController) {
        guard token != nil else { return }
        let native = controller.native
        var checks: [String] = []
        var errors: [String] = []
        var report: [String: Any] = [:]
        native.onError = { errors.append(String(describing: $0)) }
        do {
            let trigger = try button(native, "Edit profile")
            try require(native.popover(named: "Profile")?.isPresented == false && trigger.window != nil, "Initial native panel is open or detached")
            checks.append("mounted-native-popover-page")
            trigger.sendActions(for: .touchUpInside)
            guard let profile = native.popover(named: "Profile"), profile.isPresented else { throw Failure(message: "Native panel did not open") }
            let field = try input(profile.content, "Profile name")
            try require(field.isFirstResponder && field.window != nil, "Native editor did not receive first responder")
            checks.append("programmatic-popover-open")
            report["geometry"] = try geometry(native, profile, trigger)
            checks.append("native-popover-anchor-geometry")
            field.insertText("中文😀")
            field.sendActions(for: .editingChanged)
            try require(try state(native)["value"] as? String == "中文😀", "Native Unicode edit did not reach Rust")
            guard let selection = field.selectedTextRange else { throw Failure(message: "Native editor selection unavailable") }
            let selectionStart = field.offset(from: field.beginningOfDocument, to: selection.start)
            let selectionEnd = field.offset(from: field.beginningOfDocument, to: selection.end)
            native.invalidateEnvironment()
            let retainedField = try input(profile.content, "Profile name")
            guard let retainedSelection = field.selectedTextRange else { throw Failure(message: "Publication removed native selection") }
            try require(retainedField === field && field.offset(from: field.beginningOfDocument, to: retainedSelection.start) == selectionStart && field.offset(from: field.beginningOfDocument, to: retainedSelection.end) == selectionEnd, "Publication changed native editor or selection")
            checks.append("native-popover-input-to-rust")
            field.selectAll(nil)
            field.setMarkedText("zhong", selectedRange: NSRange(location: 5, length: 0))
            field.sendActions(for: .editingChanged)
            try require(try state(native)["value"] as? String == "中文😀" && field.markedTextRange != nil, "Preedit committed business state")
            native.invalidateEnvironment()
            try require(field.markedTextRange != nil && field.text == "zhong", "Publication cleared native preedit")
            field.unmarkText()
            field.sendActions(for: .editingChanged)
            try require(try state(native)["value"] as? String == "zhong", "Native preedit commit did not reach Rust")
            checks.append("native-popover-preedit-publication")
            try button(profile.content, "More options").sendActions(for: .touchUpInside)
            guard let details = native.popover(named: "Details"), details.isPresented else { throw Failure(message: "Nested native panel missing") }
            details.requestDismiss(backdrop: false)
            try require(!details.isPresented && profile.isPresented, "Nested dismissal removed wrong panel")
            try button(profile.content, "More options").sendActions(for: .touchUpInside)
            try button(details.content, "Apply option").sendActions(for: .touchUpInside)
            try require(try state(native)["count"] as? Int == 1 && !details.isPresented && profile.isPresented, "Nested contextual action failed")
            checks.append("native-popover-nested-context")
            try button(native, "Reset name").sendActions(for: .touchUpInside)
            try require(field.text == "Reset 中文 😀", "Controlled reset did not update retained editor")
            try button(profile.content, "Save profile").sendActions(for: .touchUpInside)
            try require(!profile.isPresented && profile.content.window == nil, "Context close left a native panel attached")
            trigger.sendActions(for: .touchUpInside)
            try require(try input(profile.content, "Profile name") === field, "Reopen replaced native editor")
            checks.append("native-popover-controlled-reset-retained")
            guard let scroll = descendants(native).compactMap({ $0 as? UIScrollView }).first else { throw Failure(message: "Missing native scroll") }
            let originalY = profile.content.convert(profile.content.bounds, to: native.window).minY
            scroll.setContentOffset(CGPoint(x: 0, y: 20), animated: false)
            native.invalidateEnvironment()
            let scrolledY = profile.content.convert(profile.content.bounds, to: native.window).minY
            try require(abs(scrolledY - originalY + 20) < 1, "Native popup did not follow scrolled anchor")
            checks.append("native-popover-scroll-anchor")
            try button(native, "Other panel").sendActions(for: .touchUpInside)
            guard let other = native.popover(named: "Other"), other.isPresented else { throw Failure(message: "Independent panel missing") }
            profile.requestDismiss(backdrop: false)
            try require(profile.isPresented && other.isPresented, "Older panel accepted newest-panel dismissal")
            other.requestDismiss(backdrop: true)
            try require(!other.isPresented && profile.isPresented, "Outside dismissal removed wrong panel")
            profile.requestDismiss(backdrop: false)
            checks.append("native-popover-opening-order")
            let bottom = try button(native, "Bottom panel")
            scroll.setContentOffset(CGPoint(x: 0, y: max(0, scroll.contentSize.height - scroll.bounds.height)), animated: false)
            native.invalidateEnvironment()
            bottom.sendActions(for: .touchUpInside)
            guard let bottomPanel = native.popover(named: "Bottom"), bottomPanel.isPresented, let window = native.window else { throw Failure(message: "Bottom panel did not open") }
            let bottomFrame = bottomPanel.content.convert(bottomPanel.content.bounds, to: window)
            let bottomAnchor = bottom.convert(bottom.bounds, to: window)
            try require(bottomFrame.maxY <= bottomAnchor.minY && native.convert(native.bounds, to: window).contains(bottomFrame), "Bottom panel did not flip inside viewport")
            bottomPanel.requestDismiss(backdrop: false)
            scroll.setContentOffset(.zero, animated: false)
            native.invalidateEnvironment()
            checks.append("native-popover-edge-flip")
            trigger.sendActions(for: .touchUpInside)
            let saved = try native.state()
            native.close()
            try require(!profile.isPresented && profile.content.window == nil && native.subviews.isEmpty, "Disposed native overlay remains attached")
            try native.mount()
            trigger.sendActions(for: .touchUpInside)
            profile.requestDismiss(backdrop: false)
            field.text = "stale"; field.sendActions(for: .editingChanged)
            try require(try state(native)["value"] as? String == "" && errors.isEmpty, "Old native action affected replacement root")
            checks.append("native-popover-dispose-stale-actions")
            try native.restore(saved)
            try button(native, "Edit profile").sendActions(for: .touchUpInside)
            guard let fresh = native.popover(named: "Profile") else { throw Failure(message: "Fresh native panel missing") }
            try require(try input(fresh.content, "Profile name").text == "Reset 中文 😀" && fresh !== profile, "Fresh root failed to restore business value")
            fresh.requestDismiss(backdrop: false)
            checks.append("native-popover-fresh-root-restore")
            report["passed"] = errors.isEmpty
        } catch { report["passed"] = false; report["error"] = String(describing: error) }
        report["checks"] = checks
        report["native_errors"] = errors
        write(report)
    }
}

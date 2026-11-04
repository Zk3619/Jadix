import UIKit

enum NativeChecks {
    private struct Failure: Error { let message: String }

    private static func require(_ condition: Bool, _ message: String) throws {
        if !condition { throw Failure(message: message) }
    }

    private static func descendants(_ view: UIView) -> [UIView] {
        view.subviews.flatMap { [$0] + descendants($0) }
    }

    private static func laidOut(_ view: UIView) -> Bool {
        view.window != nil && view.bounds.width.isFinite && view.bounds.height.isFinite && view.bounds.width > 0 && view.bounds.height > 0
    }

    private static func object(_ source: String) throws -> [String: Any] {
        guard let result = try JSONSerialization.jsonObject(with: Data(source.utf8)) as? [String: Any] else {
            throw Failure(message: "Invalid Rust state")
        }
        return result
    }

    private static var token: String? {
        guard let value = ProcessInfo.processInfo.environment["JADIX_NATIVE_CHECK_TOKEN"], UUID(uuidString: value) != nil else { return nil }
        return value
    }

    private static func checkDynamicType(_ controller: NativeController) throws -> [String: Any] {
        let container = UIViewController()
        let probe = JadixNativeView()
        controller.addChild(container)
        container.view.frame = controller.native.frame
        controller.view.addSubview(container.view)
        container.view.addSubview(probe)
        probe.frame = container.view.bounds
        container.didMove(toParent: controller)
        defer {
            probe.close()
            container.willMove(toParent: nil)
            container.view.removeFromSuperview()
            container.removeFromParent()
        }
        var errors: [String] = []
        probe.onError = { errors.append(String(describing: $0)) }
        controller.setOverrideTraitCollection(UITraitCollection(preferredContentSizeCategory: .large), forChild: container)
        container.view.layoutIfNeeded()
        try require(probe.traitCollection.preferredContentSizeCategory == .large, "Probe did not inherit large Dynamic Type traits")
        try probe.mount()
        let widgets = descendants(probe)
        guard let field = widgets.compactMap({ $0 as? UITextField }).first(where: { !$0.isSecureTextEntry }),
              let editor = widgets.compactMap({ $0 as? UITextView }).first,
              let button = widgets.compactMap({ $0 as? UIButton }).first,
              let caption = button.titleLabel,
              let placeholder = descendants(editor).compactMap({ $0 as? UILabel }).first(where: { $0.text?.contains(" / ") == true }) else { throw Failure(message: "Missing Dynamic Type probe controls") }
        var fontSizes: [() -> CGFloat] = [{ field.font?.pointSize ?? 0 }, { editor.font?.pointSize ?? 0 }, { caption.font.pointSize }, { placeholder.font.pointSize }]
        var retained: [UIView] = [field, editor, caption, placeholder]
        for toggle in widgets.compactMap({ $0 as? UISwitch }) {
            guard let label = toggle.superview?.subviews.compactMap({ $0 as? UILabel }).first else { throw Failure(message: "Missing boolean control caption") }
            fontSizes.append { label.font.pointSize }
            retained.append(label)
        }
        #if JADIX_NOTES
        try require(fontSizes.count == 4, "Unexpected notebook typography controls")
        #else
        try require(fontSizes.count == 6, "Missing checkbox or Switch typography control")
        #endif
        let standardFonts = fontSizes.map { $0() }
        try require(standardFonts.allSatisfy { $0 > 0 }, "Invalid standard control fonts")
        let request: [String: Any] = ["node": ["kind": "text", "text": "Dynamic Type 中文", "style": ["font_size": 17.0, "font_weight": 400.0]],
            "maxWidth": 320.0, "maxHeight": 1000.0, "widthMode": "max-content", "heightMode": "max-content"]
        let data = try JSONSerialization.data(withJSONObject: request)
        let measurement = String(decoding: data, as: UTF8.self)
        let standardMeasure = try probe.measure(measurement)
        try require(field.becomeFirstResponder(), "Dynamic Type input could not obtain focus")
        field.text = "A😀אבe\u{301}"
        field.selectedTextRange = field.textRange(from: field.endOfDocument, to: field.endOfDocument)
        field.sendActions(for: .editingChanged)
        let before = try probe.state()
        guard let selected = field.selectedTextRange else { throw Failure(message: "Missing native selection") }
        let selection = [field.offset(from: field.beginningOfDocument, to: selected.start), field.offset(from: field.beginningOfDocument, to: selected.end)]
        controller.setOverrideTraitCollection(UITraitCollection(preferredContentSizeCategory: .accessibilityExtraExtraExtraLarge), forChild: container)
        container.view.layoutIfNeeded()
        try require(probe.traitCollection.preferredContentSizeCategory == .accessibilityExtraExtraExtraLarge, "Probe did not inherit accessibility Dynamic Type traits")
        let largeMeasure = try probe.measure(measurement)
        let largeFonts = fontSizes.map { $0() }
        try require(zip(standardFonts, largeFonts).allSatisfy { $0.1 > $0.0 } && largeMeasure.height > standardMeasure.height, "Control fonts or host text measurement ignored Dynamic Type traits")
        try require(retained.allSatisfy { $0.isDescendant(of: probe) } && field.isFirstResponder && field.text == "A😀אבe\u{301}", "Dynamic Type replaced an editing control or text")
        guard let after = field.selectedTextRange else { throw Failure(message: "Dynamic Type lost selection") }
        try require([field.offset(from: field.beginningOfDocument, to: after.start), field.offset(from: field.beginningOfDocument, to: after.end)] == selection, "Dynamic Type changed UTF-16 selection")
        try require(try probe.state() == before && errors.isEmpty, "Dynamic Type generated a business edit or native error")
        controller.setOverrideTraitCollection(UITraitCollection(preferredContentSizeCategory: .large), forChild: container)
        container.view.layoutIfNeeded()
        let restoredFonts = fontSizes.map { $0() }
        let restoredMeasure = try probe.measure(measurement)
        try require(zip(standardFonts, restoredFonts).allSatisfy { abs($0.1 - $0.0) < 0.1 } && abs(restoredMeasure.height - standardMeasure.height) < 0.1, "Dynamic Type accumulated scaling on return to large")
        try require(try probe.state() == before && errors.isEmpty, "Restored typography changed business state")
        return ["categories": ["large", "accessibilityExtraExtraExtraLarge", "large"],
            "standard_fonts": standardFonts, "accessibility_fonts": largeFonts, "restored_fonts": restoredFonts,
            "measurement_heights": [standardMeasure.height, largeMeasure.height, restoredMeasure.height]]
    }

    private static func checkKeyboardViewport(_ controller: NativeController) throws -> [String: Any] {
        let native = controller.native
        guard let window = controller.view.window,
              let field = descendants(native).compactMap({ $0 as? UITextField }).last(where: { $0.isEnabled }),
              let scroll = descendants(native).compactMap({ $0 as? UIScrollView }).first(where: { !($0 is UITextView) }) else { throw Failure(message: "Missing keyboard viewport controls") }
        let center = NotificationCenter.default
        let previousInsets = controller.additionalSafeAreaInsets
        let previousOffset = scroll.contentOffset
        func layout() {
            controller.view.setNeedsLayout()
            controller.view.layoutIfNeeded()
        }
        func keyboard(_ frame: CGRect) {
            let screenFrame = window.convert(controller.view.convert(frame, to: window), to: nil)
            center.post(name: UIResponder.keyboardWillChangeFrameNotification, object: nil, userInfo: [UIResponder.keyboardFrameEndUserInfoKey: NSValue(cgRect: screenFrame)])
            layout()
        }
        defer {
            controller.view.endEditing(true)
            center.post(name: UIResponder.keyboardWillHideNotification, object: nil)
            controller.additionalSafeAreaInsets = previousInsets
            layout()
            scroll.setContentOffset(previousOffset, animated: false)
        }
        controller.view.endEditing(true)
        center.post(name: UIResponder.keyboardWillHideNotification, object: nil)
        layout()
        let baseline = native.frame
        let before = try native.state()
        try require(baseline.height > 120 && field.becomeFirstResponder(), "Invalid keyboard viewport or focus")
        let coveredHeight = min(200, baseline.height / 3)
        let docked = CGRect(x: baseline.minX, y: baseline.maxY - coveredHeight, width: baseline.width, height: coveredHeight + 40)
        keyboard(docked)
        let reduced = native.frame
        try require(abs(reduced.height - (baseline.height - coveredHeight)) < 0.5 && reduced.minY == baseline.minY && reduced.width == baseline.width, "Docked keyboard viewport mismatch")
        let fieldRect = field.convert(field.bounds, to: scroll)
        try require(scroll.bounds.insetBy(dx: -1, dy: -1).contains(fieldRect) && field.isFirstResponder, "Keyboard viewport failed to reveal the focused input")
        center.post(name: UIResponder.keyboardWillHideNotification, object: nil)
        layout()
        let hidden = native.frame
        try require(hidden == baseline, "Keyboard hide left stale viewport occlusion")
        keyboard(CGRect(x: baseline.minX, y: baseline.maxY + 40, width: baseline.width, height: coveredHeight))
        try require(native.frame == baseline, "Offscreen keyboard changed viewport")
        keyboard(docked)
        center.post(name: UIApplication.didEnterBackgroundNotification, object: nil)
        layout()
        let background = native.frame
        try require(background == baseline, "Background notification retained stale keyboard occlusion")
        center.post(name: UIApplication.didBecomeActiveNotification, object: nil)
        layout()
        let active = native.frame
        try require(active == baseline, "Active notification failed to restore viewport")
        controller.additionalSafeAreaInsets = UIEdgeInsets(top: previousInsets.top + 12, left: previousInsets.left, bottom: previousInsets.bottom + 16, right: previousInsets.right)
        layout()
        let safeArea = native.frame
        try require(abs(safeArea.minY - baseline.minY - 12) < 0.5 && abs(safeArea.height - baseline.height + 28) < 0.5, "Safe area change did not reach native viewport")
        controller.additionalSafeAreaInsets = previousInsets
        layout()
        try require(native.frame == baseline && field.isFirstResponder && (try native.state()) == before, "Viewport recovery changed focus or business state")
        return ["authority": "programmatic UIKit notifications and additional safe area; not OS background or physical keyboard",
            "heights": [baseline.height, reduced.height, hidden.height, background.height, active.height, safeArea.height, native.frame.height],
            "covered_height": coveredHeight, "safe_area_reduction": 28.0, "focused_control_visible": true]
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
            report["event_authority"] = "programmatic UIKit actions and delegates; not physical input or IME"
            let data = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
            try data.write(to: directory.appendingPathComponent("native-checks.json"), options: [.atomic])
        } catch { NSLog("Jadix checks receipt: %@", String(describing: error)) }
    }

    static func mountFailed(_ error: Error) {
        write(["passed": false, "checks": [], "error": String(describing: error)])
    }

    static func run(_ controller: NativeController) {
        guard token != nil else { return }
        let native = controller.native
        var checks: [String] = []
        var report: [String: Any] = [:]
        var errors: [String] = []
        let previousError = native.onError
        native.onError = { error in errors.append(String(describing: error)); previousError?(error) }
        defer { native.onError = previousError }
        do {
            controller.view.layoutIfNeeded()
            try require(laidOut(native), "Missing visible native viewport")
            let initial = try object(native.state())
            let widgets = descendants(native)
            try require(widgets.contains { $0 is UIScrollView } && widgets.contains { $0 is UITextField } && widgets.contains { $0 is UIButton }, "Missing platform controls")
            checks.append("mounted-platform-controls")
            let beforeTypography = try native.state()
            report["dynamic_type"] = try checkDynamicType(controller)
            try require(try native.state() == beforeTypography, "Typography probe affected the application root")
            checks.append("native-dynamic-type-traits")
            report["keyboard_viewport"] = try checkKeyboardViewport(controller)
            checks.append("native-keyboard-viewport")
            guard let editor = widgets.compactMap({ $0 as? UITextView }).first else { throw Failure(message: "Missing native multiline editor") }
            try require(laidOut(editor), "Multiline editor has no platform geometry")
            let originalText = editor.text ?? ""
            let multiline = String(repeating: "第二行 😀 אב e\u{301}\n", count: 48)
            editor.text = multiline
            editor.selectedRange = NSRange(location: 0, length: 3)
            native.textViewDidChange(editor)
            let multilineState = try object(native.state())
            #if JADIX_NOTES
            let publishedText = (multilineState["draft"] as? [String: Any])?["body"] as? String
            #else
            let publishedText = (multilineState["extensions"] as? [String: Any])?["note"] as? String
            #endif
            try require(publishedText == multiline && editor.text == multiline, "Multiline edit did not round trip through Rust")
            checks.append("native-multiline-edit-to-rust")
            editor.layoutIfNeeded()
            let contentSize = editor.contentSize
            let selection = editor.selectedRange
            try require(contentSize.height > editor.bounds.height, "Multiline fixture does not overflow its native viewport")
            native.invalidateEnvironment()
            try require(editor.contentSize == contentSize && editor.selectedRange == selection, "Outer layout overwrote native text layout or selection")
            let beforeScroll = try native.state()
            editor.setContentOffset(CGPoint(x: 0, y: max(0, contentSize.height - editor.bounds.height)), animated: false)
            native.scrollViewDidScroll(editor)
            try require(editor.contentOffset.y > 0 && editor.selectedRange == selection, "Native editor did not retain its selection while scrolling")
            try require(try native.state() == beforeScroll && errors.isEmpty, "Editor scrolling entered the outer scroll protocol")
            checks.append("native-textarea-scroll-isolated")
            editor.text = originalText
            editor.selectedRange = NSRange(location: 0, length: 0)
            native.textViewDidChange(editor)
            #if JADIX_NOTES
            report["application"] = "notes"
            report["initial_archive"] = initial["archive"]
            try require(initial["storage_pending"] as? Bool == false, "Notebook storage was not restored")
            guard let field = widgets.compactMap({ $0 as? UITextField }).first(where: { $0.placeholder?.hasPrefix("Title /") == true }),
                  let save = widgets.compactMap({ $0 as? UIButton }).first(where: { $0.title(for: .normal) == "Save note" }) else { throw Failure(message: "Missing notebook title or Save") }
            try require(laidOut(field) && laidOut(save), "Notebook controls have no platform geometry")
            try require(field.becomeFirstResponder(), "Notebook title could not obtain native focus")
            field.text = "UIKit acceptance \u{4e2d}\u{1f600}"
            field.selectedTextRange = field.textRange(from: field.endOfDocument, to: field.endOfDocument)
            field.sendActions(for: .editingChanged)
            let edited = try object(native.state())
            try require((edited["draft"] as? [String: Any])?["title"] as? String == field.text && edited["dirty"] as? Bool == true, "UIKit title edit did not reach Rust")
            checks.append("native-edit-to-rust")
            save.sendActions(for: .touchUpInside)
            let saved = try object(native.state())
            guard let archive = saved["archive"] as? [String: Any] else { throw Failure(message: "Missing saved archive") }
            let directory = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: false)
            let data = try Data(contentsOf: directory.appendingPathComponent("JadixNotes/notebook-v1.json"))
            let stored = try JSONSerialization.jsonObject(with: data) as? [String: Any]
            try require(saved["dirty"] as? Bool == false && saved["storage_pending"] as? Bool == false && stored != nil && NSDictionary(dictionary: stored!).isEqual(to: archive), "Save acknowledgement differs from protected notebook")
            report["saved_archive"] = archive
            checks.append("protected-document-save")
            #else
            report["application"] = "controls"
            try require(widgets.filter { $0 is UISwitch }.count == 2 && widgets.contains { $0 is UITextView } && widgets.filter { $0 is UIImageView }.count >= 2, "Missing extended native controls")
            guard let toggle = widgets.compactMap({ $0 as? UISwitch }).first(where: { $0.accessibilityLabel == "Airplane mode" }),
                  let checkbox = widgets.compactMap({ $0 as? UISwitch }).first(where: { $0.accessibilityLabel == "Accept" }) else { throw Failure(message: "Missing independent Switch and Checkbox semantics") }
            toggle.setOn(true, animated: false)
            toggle.sendActions(for: .valueChanged)
            let enabledState = try object(native.state())
            try require(toggle.isOn && checkbox.isOn && (enabledState["extensions"] as? [String: Any])?["checked"] as? Bool == true, "Switch did not update shared Rust boolean")
            checkbox.setOn(false, animated: false)
            checkbox.sendActions(for: .valueChanged)
            let disabledState = try object(native.state())
            try require(!toggle.isOn && !checkbox.isOn && (disabledState["extensions"] as? [String: Any])?["checked"] as? Bool == false, "Rust Checkbox publication did not update retained Switch")
            checks.append("native-switch-to-rust")
            guard let field = widgets.compactMap({ $0 as? UITextField }).first(where: { $0.placeholder?.hasPrefix("Name /") == true }),
                  let button = widgets.compactMap({ $0 as? UIButton }).first(where: { $0.title(for: .normal) == "Continue" }) else { throw Failure(message: "Missing native name or Continue") }
            try require(laidOut(field) && laidOut(button), "Native controls have no platform geometry")
            if let password = widgets.compactMap({ $0 as? UITextField }).first(where: { $0.placeholder?.hasPrefix("Password /") == true }) {
                try require(password.isSecureTextEntry, "Password field is not using native secure entry")
                password.text = "secret-密😀"
                password.sendActions(for: .editingChanged)
                let passwordState = try object(native.state())
                try require(password.text == "secret-密😀" && passwordState["value"] as? String != password.text, "Password value leaked into public sample state")
                checks.append("native-password-entry")
            } else { throw Failure(message: "Missing native password field") }
            try require(field.becomeFirstResponder(), "Name input could not obtain native focus")
            field.text = "UIKit acceptance \u{4e2d}\u{1f600}"
            field.selectedTextRange = field.textRange(from: field.endOfDocument, to: field.endOfDocument)
            field.sendActions(for: .editingChanged)
            let edited = try object(native.state())
            try require(edited["value"] as? String == field.text, "UIKit edit did not reach Rust")
            checks.append("native-edit-to-rust")
            button.sendActions(for: .touchUpInside)
            let activated = try object(native.state())
            try require(activated["count"] as? Int == (initial["count"] as? Int ?? -1) + 1, "UIKit button did not reach Rust")
            checks.append("native-activate-to-rust")
            #endif
            let savedState = try native.state()
            native.close()
            try require(native.subviews.isEmpty, "Disposed platform widgets survived")
            do { _ = try native.state(); throw Failure(message: "Disposed root still replies") }
            catch let error as Failure { throw error }
            catch { checks.append("dispose-rejects-state") }
            try native.mount()
            controller.didMount()
            #if JADIX_NOTES
            let restored = try object(native.state())["archive"] as? [String: Any]
            let expected = try object(savedState)["archive"] as? [String: Any]
            try require(restored != nil && expected != nil && NSDictionary(dictionary: restored!).isEqual(to: expected!), "Fresh root failed to restore notebook document")
            #else
            try native.restore(savedState)
            let restored = try object(native.state())
            let expected = try object(savedState)
            try require(NSDictionary(dictionary: restored).isEqual(to: expected), "Fresh root failed state round trip")
            #endif
            checks.append("fresh-root-restore")
            let before = try native.state()
            for widget in widgets.compactMap({ $0 as? UIButton }) { widget.sendActions(for: .touchUpInside) }
            try require(try native.state() == before && errors.isEmpty, "Detached control affected the fresh root or native error occurred")
            checks.append("stale-native-actions-isolated")
            report["passed"] = true
        } catch { report["passed"] = false; report["error"] = String(describing: error) }
        report["checks"] = checks
        report["native_errors"] = errors
        write(report)
    }
}

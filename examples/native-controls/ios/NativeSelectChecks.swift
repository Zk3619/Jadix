import UIKit

enum NativeSelectChecks {
    private struct Failure: Error { let message: String }
    private static func require(_ condition: Bool, _ message: String) throws {
        if !condition { throw Failure(message: message) }
    }
    private static func descendants(_ view: UIView) -> [UIView] {
        view.subviews.flatMap { [$0] + descendants($0) }
    }
    private static func choice(_ native: JadixNativeView, _ name: String = "Choose fruit") throws -> NativeSelect {
        guard let control = descendants(native).compactMap({ $0 as? NativeSelect }).first(where: { $0.accessibilityLabel == name }) else {
            throw Failure(message: "Missing native Select: " + name)
        }
        return control
    }
    private static func object(_ source: String) throws -> [String: Any] {
        guard let value = try JSONSerialization.jsonObject(with: Data(source.utf8)) as? [String: Any] else {
            throw Failure(message: "Invalid Rust state")
        }
        return value
    }
    private static func requireValue(_ native: JadixNativeView, _ expected: String) throws {
        try require(try object(native.state())["value"] as? String == expected, "Select callback did not reach Rust")
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
            report["application"] = "selection"
            report["event_authority"] = "programmatic Select callback and UIKit menu inspection; not menu touch, keyboard or VoiceOver"
            let data = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
            try data.write(to: directory.appendingPathComponent("native-checks.json"), options: [.atomic])
        } catch { NSLog("Jadix Select checks receipt: %@", String(describing: error)) }
    }
    static func mountFailed(_ error: Error) {
        write(["passed": false, "checks": [], "native_errors": [], "error": String(describing: error)])
    }
    private static func dynamicType(_ controller: NativeController) throws -> [String: Any] {
        let container = UIViewController()
        let native = JadixNativeView()
        controller.addChild(container)
        container.view.frame = controller.native.frame
        controller.view.addSubview(container.view)
        container.view.addSubview(native)
        native.frame = container.view.bounds
        container.didMove(toParent: controller)
        defer {
            native.close()
            container.willMove(toParent: nil)
            container.view.removeFromSuperview()
            container.removeFromParent()
        }
        var errors: [String] = []
        native.onError = { errors.append(String(describing: $0)) }
        controller.setOverrideTraitCollection(UITraitCollection(preferredContentSizeCategory: .large), forChild: container)
        container.view.layoutIfNeeded()
        try native.mount()
        let primary = try choice(native)
        let disabled = try choice(native, "Disabled fruit")
        var sizes: [[CGFloat]] = []
        var heights: [CGFloat] = []
        for category in [UIContentSizeCategory.large, .accessibilityExtraExtraExtraLarge, .large] {
            controller.setOverrideTraitCollection(UITraitCollection(preferredContentSizeCategory: category), forChild: container)
            container.view.layoutIfNeeded()
            native.invalidateEnvironment()
            try require(native.traitCollection.preferredContentSizeCategory == category, "Select did not inherit Dynamic Type trait")
            let currentPrimary = try choice(native)
            let currentDisabled = try choice(native, "Disabled fruit")
            try require(currentPrimary === primary && currentDisabled === disabled, "Typography replaced Select identity")
            sizes.append([primary.titleLabel?.font.pointSize ?? 0, disabled.titleLabel?.font.pointSize ?? 0])
            heights.append(primary.sizeThatFits(CGSize(width: 300, height: 1000)).height)
        }
        try require(errors.isEmpty, "Dynamic Type produced native errors")
        for index in 0..<2 {
            try require(sizes[1][index] > sizes[0][index] && abs(sizes[2][index] - sizes[0][index]) < 0.1, "Select font did not scale and restore")
        }
        try require(heights[1] > heights[0] && abs(heights[2] - heights[0]) < 0.1, "Select intrinsic measurement did not scale and restore")
        return ["categories": ["large", "accessibilityExtraExtraExtraLarge", "large"],
                "standard_fonts": sizes[0], "accessibility_fonts": sizes[1], "restored_fonts": sizes[2], "measurement_heights": heights]
    }
    static func run(_ controller: NativeController) {
        guard token != nil else { return }
        let native = controller.native
        var report: [String: Any] = [:]
        var checks: [String] = []
        var errors: [String] = []
        native.onError = { errors.append(String(describing: $0)) }
        do {
            let primary = try choice(native)
            let disabled = try choice(native, "Disabled fruit")
            guard let initialMenu = primary.menu else { throw Failure(message: "Native Select has no UIMenu") }
            let actions = initialMenu.children.compactMap { $0 as? UIAction }
            try require(primary.window != nil && primary.bounds.width > 0 && primary.bounds.height > 0, "Native Select is not laid out")
            try require(primary.showsMenuAsPrimaryAction && actions.count == 3 && !disabled.isEnabled, "Invalid native menu or disabled control")
            try require(actions[0].title == "Apple / 苹果" && actions[2].attributes.contains(.disabled), "Native option label or disabled state missing")
            try require(primary.title(for: .normal) == "Choose fruit" && errors.isEmpty, "Initial controlled prompt mutated state")
            checks.append("mounted-native-select-menu")
            report["controls"] = 2
            report["options"] = actions.count
            primary.requestSelection("apple")
            try requireValue(native, "apple")
            try require(primary.title(for: .normal) == "Apple / 苹果" && primary.accessibilityValue == "Apple / 苹果", "Controlled native label not updated")
            try require(primary.menu?.children.compactMap({ $0 as? UIAction }).first?.state == .on, "Native menu checked state not updated")
            primary.requestSelection("banana")
            try requireValue(native, "banana")
            checks.append("programmatic-select-to-rust")
            primary.requestSelection("blocked")
            try requireValue(native, "banana")
            try require(errors.count == 1 && primary.title(for: .normal) == "Banana / 香蕉", "Disabled option callback was not rejected/restored")
            errors.removeAll()
            checks.append("disabled-option-rejected")
            disabled.requestSelection("banana")
            try requireValue(native, "banana")
            try require(errors.count == 1 && disabled.title(for: .normal) == "Apple / 苹果", "Disabled Select callback was not rejected/restored")
            errors.removeAll()
            checks.append("disabled-select-rejected")
            guard let increment = descendants(native).compactMap({ $0 as? UIButton }).first(where: { $0.title(for: .normal) == "Continue" }),
                  let reset = descendants(native).compactMap({ $0 as? UIButton }).first(where: { $0.title(for: .normal) == "Reset selection" }) else {
                throw Failure(message: "Missing shared buttons")
            }
            let retainedMenu = primary.menu
            increment.sendActions(for: .touchUpInside)
            native.invalidateEnvironment()
            try require(try object(native.state())["count"] as? Int == 1, "Shared activation did not reach Rust")
            let retainedPrimary = try choice(native)
            try require(retainedPrimary === primary && primary.menu === retainedMenu, "Unrelated publication replaced Select or menu")
            try requireValue(native, "banana")
            checks.append("native-select-identity-retained")
            reset.sendActions(for: .touchUpInside)
            try requireValue(native, "")
            try require(primary.title(for: .normal) == "Choose fruit" && errors.isEmpty, "Controlled reset generated a rejected empty event")
            primary.requestSelection("apple")
            try requireValue(native, "apple")
            checks.append("controlled-select-reset")
            let saved = try native.state()
            var invalid = try object(saved)
            invalid["count"] = 99
            invalid["value"] = "missing"
            let data = try JSONSerialization.data(withJSONObject: invalid)
            var rejected = false
            do { try native.restore(String(decoding: data, as: UTF8.self)) } catch { rejected = true }
            let afterRejectedRestore = try native.state()
            try require(rejected && afterRejectedRestore == saved, "Invalid saved Select mutated business state")
            checks.append("invalid-select-restore-atomic")
            report["dynamic_type"] = try dynamicType(controller)
            checks.append("native-select-dynamic-type")
            native.close()
            try require(primary.menu == nil && native.subviews.isEmpty && primary.onSelect == nil, "Disposed native menu or callback survived")
            do { _ = try native.state(); throw Failure(message: "Disposed root still replies") }
            catch let error as Failure { throw error }
            catch {}
            try native.mount()
            primary.requestSelection("banana")
            disabled.requestSelection("banana")
            try requireValue(native, "")
            try require(errors.isEmpty, "Old Select callback affected replacement root")
            checks.append("select-dispose-and-stale-callbacks")
            try native.restore(saved)
            try require(try object(native.state())["value"] as? String == "apple", "Replacement Select did not restore controlled value")
            try require(try choice(native).title(for: .normal) == "Apple / 苹果", "Replacement native label did not restore")
            checks.append("fresh-select-root-restore")
            report["passed"] = true
        } catch { report["passed"] = false; report["error"] = String(describing: error) }
        report["checks"] = checks
        report["native_errors"] = errors
        write(report)
    }
}

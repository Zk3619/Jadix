import UIKit
import WebKit
import AVKit

@MainActor public protocol JadixEmbeddedView: AnyObject {
    var view: UIView { get }
    func update(_ properties: [String: Any], emit: @escaping (String, Any) -> Void)
    func dispose()
}
public typealias JadixEmbeddedViewFactory = @MainActor () -> any JadixEmbeddedView

// Only UIKit control ownership lives here. Wire validation, event routing and
// asynchronous Web/media lifetimes are shared with the other Apple host.
@MainActor final class JadixHostedView: UIView, UITextViewDelegate, UIColorPickerViewControllerDelegate {
    var emit: ((String, Any) -> Void)?
    var documentChanged: ((Bool) -> Void)?
    var resourceAllowed: ((URL) -> Bool)?
    var resourceProvider: JadixWebResourceProvider?
    weak var remoteImages: AppleRemoteImages?
    var embeddedFactory: ((String) -> JadixEmbeddedViewFactory?)?
    private(set) var documentGeneration = UUID()
    private var generation = UUID()
    private var specification: NSDictionary?
    let content = UIView()
    private var surface: UIView?
    private var web: JadixHostedWeb?
    private var media: JadixHostedMedia?
    private var shader: JadixHostedShader?
    private var terminal: JadixTerminalView?
    private var embedded: (any JadixEmbeddedView)?
    private var embeddedKind: String?
    private var appliedHTML: String?
    private var updating = false
    private var picker: UIColorPickerViewController?
    override var intrinsicContentSize: CGSize {
        let size = surface?.intrinsicContentSize ?? CGSize(width: 320, height: 180)
        return CGSize(width: size.width > 0 ? size.width : 320, height: size.height > 0 ? size.height : 180)
    }
    override func sizeThatFits(_ size: CGSize) -> CGSize { surface?.sizeThatFits(size) ?? intrinsicContentSize }
    override func layoutSubviews() {
        super.layoutSubviews(); content.frame = bounds; surface?.frame = bounds
        if let button = surface as? JadixMenuButton {
            button.setTitle(content.subviews.isEmpty ? specification?["label"] as? String : nil, for: .normal)
        }
        attachMedia()
    }
    override func didMoveToWindow() { super.didMoveToWindow(); attachMedia() }
    private func attachMedia() {
        guard let media, media.controller.parent == nil, window != nil, let parent = presenter else { return }
        parent.addChild(media.controller); media.controller.didMove(toParent: parent)
    }
    private var presenter: UIViewController? {
        var next: UIResponder? = self
        while let responder = next { if let controller = responder as? UIViewController { return controller }; next = responder.next }
        return nil
    }
    private func install(_ view: UIView) {
        guard surface !== view else { return }
        if content.superview == nil {
            content.isUserInteractionEnabled = false; content.accessibilityElementsHidden = true
            insertSubview(content, at: 0)
        }
        surface?.removeFromSuperview(); surface = view; addSubview(view); view.frame = bounds; content.frame = bounds
    }
    func update(_ value: [String: Any]) {
        let next = NSDictionary(dictionary: value)
        guard specification?.isEqual(next) != true else { return }
        generation = UUID()
        if specification?["type"] as? String != value["type"] as? String { dispose() }
        specification = value as NSDictionary
        updating = true; defer { updating = false; invalidateIntrinsicContentSize() }
        switch value["type"] as? String {
        case "web":
            if web == nil {
                let host = JadixHostedWeb()
                host.resourceAllowed = { [weak self] in self?.resourceAllowed?($0) == true }
                host.resources.provider = { [weak self] request, reply in self?.resourceProvider?(request, reply) }
                host.emit = { [weak self] event, data in self?.emit?(event, data) }
                host.documentChanged = { [weak self] ready in
                    guard let self else { return }; if !ready { self.documentGeneration = UUID() }; self.documentChanged?(ready)
                }
                web = host; install(host.view)
            }
            web?.update(value)
        case "media":
            if media == nil { let host = JadixHostedMedia(); host.emit = { [weak self] in self?.emit?($0, $1) }; media = host; install(host.view); attachMedia() }
            media?.update(value)
        case "shader":
            if shader == nil { let host = JadixHostedShader(); host.reportError = { [weak self] in self?.emit?("error", $0) }; shader = host; install(host.view) }
            shader?.update(value)
        case "canvas":
            let canvas = surface as? JadixHostedCanvas ?? JadixHostedCanvas(); install(canvas)
            canvas.commands = value["commands"] as? [[String: Any]] ?? []; canvas.setNeedsDisplay()
        case "terminal":
            if terminal == nil { let host = JadixTerminalView(); host.emit = { [weak self] in self?.emit?($0, $1) }; terminal = host; install(host) }
            terminal?.update(value)
        case "custom":
            guard let kind = value["kind"] as? String, let properties = value["properties"] as? [String: Any] else { return }
            if embeddedKind != kind { generation = UUID(); embedded?.dispose(); embedded = nil; embeddedKind = kind; surface?.removeFromSuperview(); surface = nil }
            if embedded == nil {
                guard let factory = embeddedFactory?(kind) else { emit?("error", "Unregistered native view: \(kind)"); return }
                let instance = factory()
                guard instance.view.superview == nil else { instance.dispose(); emit?("error", "Native view is already attached"); return }
                embedded = instance; install(instance.view)
            }
            let token = generation
            embedded?.update(properties) { [weak self] event, data in guard let self, self.generation == token else { return }; self.emit?(event, data) }
        case "menu_button":
            let button = surface as? JadixMenuButton ?? JadixMenuButton(type: .system); install(button)
            button.setTitle(value["label"] as? String, for: .normal); button.isEnabled = value["enabled"] as? Bool == true
            button.accessibilityLabel = value["accessibility_label"] as? String ?? value["label"] as? String
            let token = generation
            button.updateMenu(value["items"] as? [[String: Any]] ?? []) { [weak self] id in
                guard let self, self.generation == token, !self.updating else { return }; self.emit?("select", id)
            }
        case "tab_bar":
            let control = surface as? JadixTabBar ?? JadixTabBar(); install(control)
            let token = generation
            control.update(value) { [weak self] id in
                guard let self, self.generation == token, !self.updating else { return }; self.emit?("select", id)
            }
        case "segments":
            let control = surface as? JadixSegmentControl ?? JadixSegmentControl(); install(control)
            control.removeTarget(self, action: #selector(segment(_:)), for: .valueChanged); control.removeAllSegments()
            control.isEnabled = value["enabled"] as? Bool == true
            control.selectedSegmentIndex = UISegmentedControl.noSegment
            for (index, item) in (value["items"] as? [[String: Any]] ?? []).enumerated() {
                if let image = image(item) { control.insertSegment(with: image, at: index, animated: false) }
                else { control.insertSegment(withTitle: item["label"] as? String, at: index, animated: false) }
                control.setEnabled(item["enabled"] as? Bool == true, forSegmentAt: index)
                if item["id"] as? String == value["selected"] as? String { control.selectedSegmentIndex = index }
            }
            control.installAccessibility(value["items"] as? [[String: Any]] ?? [])
            control.accessibilityLabel = value["accessibility_label"] as? String
            control.addTarget(self, action: #selector(segment(_:)), for: .valueChanged)
        case "time":
            let control = surface as? JadixTimePicker ?? JadixTimePicker(); install(control)
            control.update(value); control.changed = { [weak self] in self?.emit?("change", $0) }
        case "date":
            let control = surface as? UIDatePicker ?? UIDatePicker(); install(control)
            control.removeTarget(self, action: #selector(date(_:)), for: .valueChanged)
            var calendar = Calendar(identifier: .gregorian); calendar.timeZone = TimeZone(secondsFromGMT: 0)!
            control.calendar = calendar; control.timeZone = calendar.timeZone
            control.locale = (value["locale"] as? String).map(Locale.init(identifier:)) ?? .current
            control.datePickerMode = value["type"] as? String == "time" ? .time : .date
            control.preferredDatePickerStyle = value["mode"] as? String == "calendar" ? .inline : .compact
            control.isEnabled = value["enabled"] as? Bool == true
            func decode(_ value: Any?) -> Date? {
                guard let d = value as? [String: Int] else { return nil }
                return calendar.date(from: DateComponents(year: d["year"] ?? 2000, month: d["month"] ?? 1, day: d["day"] ?? 1,
                    hour: d["hour"] ?? 12, minute: d["minute"] ?? 0, second: d["second"] ?? 0))
            }
            control.minimumDate = decode(value["minimum"]); control.maximumDate = decode(value["maximum"])
            if let date = decode(value["value"]) { control.date = date }
            control.addTarget(self, action: #selector(date(_:)), for: .valueChanged)
        case "color":
            let button = surface as? JadixButton ?? JadixButton(type: .system); install(button)
            button.removeTarget(self, action: #selector(color), for: .touchUpInside); button.addTarget(self, action: #selector(color), for: .touchUpInside)
            button.isEnabled = value["enabled"] as? Bool == true; button.setTitle(value["label"] as? String ?? "Color", for: .normal)
            button.backgroundColor = JadixVisualStyle.color(value["rgba"])
        case "rich_editor":
            let editor = surface as? JadixRichEditor ?? JadixRichEditor(frame: .zero, textContainer: nil)
            install(editor); editor.remoteImages = remoteImages
            editor.emit = { [weak self] in self?.emit?($0, $1) }; editor.update(value)
        default: emit?("error", "Unsupported hosted specification")
        }
    }
    private func image(_ item: [String: Any]) -> UIImage? {
        let icon = item["icon"] as? [String: Any]
        return (item["symbol"] as? String ?? icon?["source"] as? String).flatMap { UIImage(systemName: $0.replacingOccurrences(of: "sf:", with: "")) }
    }
    @objc private func segment(_ control: UISegmentedControl) {
        guard !updating, let items = specification?["items"] as? [[String: Any]], items.indices.contains(control.selectedSegmentIndex),
              let id = items[control.selectedSegmentIndex]["id"] as? String else { return }; emit?("select", id)
    }
    @objc private func date(_ control: UIDatePicker) {
        guard !updating else { return }; let p = control.calendar.dateComponents([.year, .month, .day, .hour, .minute, .second], from: control.date)
        if specification?["type"] as? String == "time" { emit?("change", ["hour": p.hour ?? 0, "minute": p.minute ?? 0, "second": specification?["seconds"] as? Bool == true ? p.second ?? 0 : 0]) }
        else { emit?("change", ["year": p.year ?? 2000, "month": p.month ?? 1, "day": p.day ?? 1]) }
    }
    @objc private func color() {
        guard picker == nil, let presenter else { return }; let controller = UIColorPickerViewController()
        controller.supportsAlpha = specification?["alpha"] as? Bool == true
        controller.selectedColor = JadixVisualStyle.color(specification?["rgba"]) ?? .clear; controller.delegate = self; picker = controller
        if let popover = controller.popoverPresentationController { popover.sourceView = self; popover.sourceRect = bounds }
        presenter.present(controller, animated: true)
    }
    func colorPickerViewControllerDidSelectColor(_ viewController: UIColorPickerViewController) {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        guard viewController.selectedColor.getRed(&r, green: &g, blue: &b, alpha: &a) else { return }
        emit?("change", [r, g, b, a].map { Int((min(1, max(0, $0)) * 255).rounded()) })
    }
    func colorPickerViewControllerDidFinish(_ viewController: UIColorPickerViewController) { picker = nil }
    func evaluate(_ script: String, completion: @escaping (Any?, Error?) -> Void) {
        guard let web else { completion(nil, URLError(.unsupportedURL)); return }; web.view.evaluateJavaScript(script, completionHandler: completion)
    }
    func dispose() {
        (surface as? JadixMenuButton)?.retire()
        (surface as? JadixTabBar)?.retire()
        generation = UUID(); documentGeneration = UUID(); web?.dispose(); web = nil
        media?.controller.willMove(toParent: nil); media?.dispose(); media?.controller.removeFromParent(); media = nil
        shader?.dispose(); shader = nil; terminal?.dispose(); terminal = nil
        embedded?.dispose(); embedded = nil; embeddedKind = nil
        (surface as? JadixRichEditor)?.dispose()
        picker?.delegate = nil; picker?.dismiss(animated: false); picker = nil
        surface?.removeFromSuperview(); surface = nil; specification = nil; appliedHTML = nil
    }
}

private final class JadixHostedCanvas: UIView {
    var commands: [[String: Any]] = []
    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext() else { return }
        for command in commands {
            let color = JadixVisualStyle.color(command["color"]) ?? .clear
            switch command["type"] as? String {
            case "fill":
                if let r = command["rect"] as? [Double], r.count == 4 { context.setFillColor(color.cgColor); context.fill(CGRect(x: r[0], y: r[1], width: r[2], height: r[3])) }
            case "stroke":
                guard let points = command["points"] as? [[Double]], let first = points.first, first.count == 2 else { continue }
                context.beginPath(); context.move(to: CGPoint(x: first[0], y: first[1]))
                for p in points.dropFirst() where p.count == 2 { context.addLine(to: CGPoint(x: p[0], y: p[1])) }
                if command["closed"] as? Bool == true { context.closePath() }
                context.setStrokeColor(color.cgColor); context.setLineWidth(command["width"] as? Double ?? 1); context.strokePath()
            case "text":
                if let text = command["text"] as? String, let p = command["position"] as? [Double], p.count == 2 {
                    (text as NSString).draw(at: CGPoint(x: p[0], y: p[1]), withAttributes: [.font: UIFont.systemFont(ofSize: command["size"] as? Double ?? 14), .foregroundColor: color])
                }
            default: break
            }
        }
    }
}

@MainActor private final class JadixTimePicker: UIPickerView, UIPickerViewDataSource, UIPickerViewDelegate {
    private var seconds = false
    var changed: (([String: Int]) -> Void)?
    override init(frame: CGRect) { super.init(frame: frame); dataSource = self; delegate = self }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    func update(_ value: [String: Any]) {
        seconds = value["seconds"] as? Bool == true; reloadAllComponents()
        isUserInteractionEnabled = value["enabled"] as? Bool == true; alpha = isUserInteractionEnabled ? 1 : 0.45
        let time = value["value"] as? [String: Int] ?? [:]
        for (component, key) in ["hour", "minute", "second"].enumerated() where component < numberOfComponents {
            selectRow(min(component == 0 ? 23 : 59, max(0, time[key] ?? 0)), inComponent: component, animated: false)
        }
    }
    func numberOfComponents(in pickerView: UIPickerView) -> Int { seconds ? 3 : 2 }
    func pickerView(_ pickerView: UIPickerView, numberOfRowsInComponent component: Int) -> Int { component == 0 ? 24 : 60 }
    func pickerView(_ pickerView: UIPickerView, titleForRow row: Int, forComponent component: Int) -> String? { String(format: "%02d", row) }
    func pickerView(_ pickerView: UIPickerView, accessibilityLabelForComponent component: Int) -> String? {
        NSLocalizedString(["Hours", "Minutes", "Seconds"][component], comment: "Time picker component")
    }
    func pickerView(_ pickerView: UIPickerView, didSelectRow row: Int, inComponent component: Int) {
        changed?(["hour": selectedRow(inComponent: 0), "minute": selectedRow(inComponent: 1), "second": seconds ? selectedRow(inComponent: 2) : 0])
    }
}

@MainActor private final class JadixSegmentControl: UISegmentedControl {
    private var entries: [JadixSegmentAccessibility] = []
    override var canBecomeFirstResponder: Bool { isEnabled }
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard isEnabled, let key = presses.first?.key,
              key.modifierFlags.intersection([.command, .control, .alternate]).isEmpty else { super.pressesBegan(presses, with: event); return }
        let step: Int
        switch key.keyCode {
        case .keyboardLeftArrow: step = effectiveUserInterfaceLayoutDirection == .rightToLeft ? 1 : -1
        case .keyboardRightArrow: step = effectiveUserInterfaceLayoutDirection == .rightToLeft ? -1 : 1
        case .keyboardHome: step = 1
        case .keyboardEnd: step = -1
        default: super.pressesBegan(presses, with: event); return
        }
        guard numberOfSegments > 0 else { return }
        var index = key.keyCode == .keyboardHome ? -1 : key.keyCode == .keyboardEnd ? numberOfSegments : selectedSegmentIndex
        if index == UISegmentedControl.noSegment, step < 0 { index = numberOfSegments }
        for _ in 0..<numberOfSegments {
            index = (index + step + numberOfSegments) % numberOfSegments
            if isEnabledForSegment(at: index) {
                if selectedSegmentIndex != index { selectedSegmentIndex = index; sendActions(for: .valueChanged); setNeedsLayout() }
                return
            }
        }
    }
    func installAccessibility(_ items: [[String: Any]]) {
        isAccessibilityElement = false
        entries = items.enumerated().map { index, item in
            let element = JadixSegmentAccessibility(accessibilityContainer: self); element.owner = self; element.index = index
            element.accessibilityLabel = item["accessibility_label"] as? String ?? item["label"] as? String
            return element
        }
        accessibilityElements = entries; setNeedsLayout()
    }
    override func layoutSubviews() {
        super.layoutSubviews()
        let width = bounds.width / CGFloat(max(1, entries.count))
        for element in entries {
            let index = effectiveUserInterfaceLayoutDirection == .rightToLeft ? entries.count - 1 - element.index : element.index
            element.accessibilityFrameInContainerSpace = CGRect(x: CGFloat(index) * width, y: 0, width: width, height: bounds.height)
            element.accessibilityTraits = isEnabled && isEnabledForSegment(at: element.index) ? [.button] : [.button, .notEnabled]
            if selectedSegmentIndex == element.index { element.accessibilityTraits.insert(.selected) }
        }
    }
}
@MainActor private final class JadixSegmentAccessibility: UIAccessibilityElement {
    weak var owner: JadixSegmentControl?
    var index = 0
    override func accessibilityActivate() -> Bool {
        guard let owner, owner.isEnabled, owner.isEnabledForSegment(at: index) else { return false }
        owner.selectedSegmentIndex = index; owner.sendActions(for: .valueChanged); owner.setNeedsLayout(); return true
    }
}

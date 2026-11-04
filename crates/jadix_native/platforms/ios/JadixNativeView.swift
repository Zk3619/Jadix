import UIKit
import ImageIO

private let measureNative: JadixNativeMeasure = { context, json, output in
    precondition(Thread.isMainThread)
    guard let context = context, let json = json, let output = output else { return false }
    let host = Unmanaged<JadixNativeView>.fromOpaque(context).takeUnretainedValue()
    do {
        let size = try host.measure(String(cString: json))
        output[0] = Double(size.width)
        output[1] = Double(size.height)
        return true
    } catch { return false }
}

private let publishNative: JadixNativePublish = { context, json in
    precondition(Thread.isMainThread)
    guard let context = context, let json = json else { return false }
    let host = Unmanaged<JadixNativeView>.fromOpaque(context).takeUnretainedValue()
    do { try host.publish(String(cString: json)); return true }
    catch { return false }
}

private let clearNative: JadixNativeClear = { context in
    precondition(Thread.isMainThread)
    guard let context = context else { return }
    Unmanaged<JadixNativeView>.fromOpaque(context).takeUnretainedValue().clear()
}

private enum NativeError: Error { case invalid(String) }

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
        toggle.frame = CGRect(x: 0, y: (bounds.height - size.height) / 2, width: size.width, height: size.height)
        caption.frame = CGRect(x: size.width + 8, y: 0, width: max(0, bounds.width - size.width - 8), height: bounds.height)
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
        content.clipsToBounds = true
        content.accessibilityViewIsModal = true
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        backdrop.frame = view.bounds
    }
    override var canBecomeFirstResponder: Bool { true }
    override var keyCommands: [UIKeyCommand]? {
        [UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(cancelDialog))]
    }
    override func accessibilityPerformEscape() -> Bool { dismissDialog(false); return true }
    @objc private func outside() { dismissDialog(true) }
    @objc private func cancelDialog() { dismissDialog(false) }
}

final class NativeSelect: UIButton {
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

    func show(in host: UIView, frame: CGRect, name: String) throws {
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
            window.rootViewController = NativeModalController(content: content) { [weak self] backdrop in self?.dismissDialog?(backdrop) }
            modalWindow = window
            window.makeKeyAndVisible()
            UIAccessibility.post(notification: .screenChanged, argument: content)
        }
        guard let window = modalWindow else { return }
        window.rootViewController?.loadViewIfNeeded()
        content.frame = host.convert(frame, to: window)
        content.accessibilityLabel = name
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
    private var root: UInt64 = 0
    private var environment: UInt64 = 1
    private var publishing = false
    private var views: [String: UIView] = [:]
    private var descriptors: [String: [String: Any]] = [:]
    private var identities: [ObjectIdentifier: String] = [:]
    private var revisions: [String: UInt64] = [:]
    private var modalOrder: [String] = []
    private var popoverOrder: [String] = []
    private var popoverUpdatePending = false
    private var lastSize = CGSize.zero
    private var appliedContentSizeCategory: UIContentSizeCategory?
    public var onError: ((Error) -> Void)?
    public var onStateChange: ((String) -> Void)?

    public func mount() throws {
        precondition(Thread.isMainThread)
        guard root == 0 else { throw NativeError.invalid("root already mounted") }
        root = jadix_native_create(Unmanaged.passUnretained(self).toOpaque(), measureNative, publishNative,
                                   clearNative, Float(bounds.width), Float(bounds.height))
        guard root != 0 else { throw NativeError.invalid("native mount failed") }
        lastSize = bounds.size
        appliedContentSizeCategory = traitCollection.preferredContentSizeCategory
        schedulePopovers()
    }

    public func close() {
        precondition(Thread.isMainThread)
        onStateChange = nil
        let previous = root
        root = 0
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

    deinit { if root != 0 { jadix_native_dispose(root) } }

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
        if window == nil {
            for id in popoverOrder.reversed() { (views[id] as? NativePopover)?.hide(restore: false) }
        } else { schedulePopovers() }
    }

    private func revealFocusedControl() {
        guard !publishing, root != 0, let focused = views.values.first(where: { $0.isFirstResponder }) else { return }
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
        if previousTraitCollection?.preferredContentSizeCategory != traitCollection.preferredContentSizeCategory ||
           previousTraitCollection?.displayScale != traitCollection.displayScale {
            invalidateEnvironment()
        }
    }

    public func invalidateEnvironment() {
        precondition(Thread.isMainThread)
        guard root != 0 else { return }
        do {
            let category = traitCollection.preferredContentSizeCategory
            if appliedContentSizeCategory != category {
                let previousPublishing = publishing
                publishing = true
                defer { publishing = previousPublishing }
                for (id, view) in views {
                    if let node = descriptors[id] { try applyTypography(node, to: view) }
                }
                appliedContentSizeCategory = category
            }
            environment += 1
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
        case "box": return UIView()
        case "dialog": return NativeModal()
        case "popover": return NativePopover(frame: .zero)
        case "scroll": let view = UIScrollView(); view.delegate = self; return view
        case "text": let view = UILabel(); view.numberOfLines = 0; return view
        case "image":
            let view = UIImageView()
            view.contentMode = .scaleAspectFit
            view.isAccessibilityElement = true
            return view
        case "button":
            let view = UIButton(type: .system)
            view.addTarget(self, action: #selector(activate(_:)), for: .touchUpInside)
            return view
        case "select": return NativeSelect(type: .system)
        case "input":
            let view = UITextField()
            view.borderStyle = .roundedRect
            view.delegate = self
            view.addTarget(self, action: #selector(edited(_:)), for: .editingChanged)
            view.addTarget(self, action: #selector(focused(_:)), for: .editingDidBegin)
            return view
        case "textarea":
            let view = NativeTextView(frame: .zero, textContainer: nil)
            view.delegate = self
            return view
        case "checkbox", "switch":
            let view = NativeCheckbox(frame: .zero)
            view.addTarget(self, action: #selector(toggled(_:)), for: .valueChanged)
            return view
        default: throw NativeError.invalid("unsupported UIKit node")
        }
    }

    private func color(_ value: Any?) -> UIColor {
        guard let channels = value as? [Double], channels.count == 4 else { return .clear }
        return UIColor(red: CGFloat(channels[0] / 255), green: CGFloat(channels[1] / 255),
                       blue: CGFloat(channels[2] / 255), alpha: CGFloat(channels[3] / 255))
    }

    private func applyTypography(_ node: [String: Any], to view: UIView) throws {
        guard let style = node["style"] as? [String: Any], let size = style["font_size"] as? Double,
              let weight = style["font_weight"] as? Double else { throw NativeError.invalid("invalid native style") }
        let systemFont = UIFont.systemFont(ofSize: CGFloat(size), weight: weight >= 600 ? .semibold : .regular)
        var selectedFont = systemFont
        for raw in (style["font_family"] as? String ?? "system-ui").split(separator: ",") {
            let family = raw.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines.union(CharacterSet(charactersIn: "\"'")))
            if ["system-ui", "ui-sans-serif", "sans-serif"].contains(family) { selectedFont = systemFont; break }
            if ["monospace", "ui-monospace"].contains(family) {
                selectedFont = UIFont.monospacedSystemFont(ofSize: CGFloat(size), weight: weight >= 600 ? .semibold : .regular); break
            }
            if let font = UIFont(name: family, size: CGFloat(size)) { selectedFont = font; break }
        }
        let font = UIFontMetrics.default.scaledFont(for: selectedFont, compatibleWith: traitCollection)
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
        if let imageView = view as? UIImageView {
            guard let source = node["source"] as? String,
                  source.hasPrefix("data:image/png;base64,"),
                  let data = Data(base64Encoded: String(source.dropFirst("data:image/png;base64,".count))) else { throw NativeError.invalid("invalid native PNG image") }
            let decoded = CGImageSourceCreateWithData(data as CFData, nil).flatMap {
                CGImageSourceCreateImageAtIndex($0, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary)
            }
            imageView.image = decoded.map { UIImage(cgImage: $0) } ?? UIImage(systemName: "photo")
            imageView.accessibilityValue = decoded == nil ? "Image unavailable" : nil
            if decoded == nil { imageView.backgroundColor = UIColor(white: 238.0 / 255.0, alpha: 1) }
            imageView.accessibilityLabel = node["name"] as? String ?? node["alt"] as? String
        }
        if let label = view as? UILabel {
            label.text = node["text"] as? String
            label.textColor = color(style["foreground"])
        }
        if let button = view as? UIButton {
            button.setTitle(node["text"] as? String, for: .normal)
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
        if let select = view as? NativeSelect { try select.configure(node) }
        if let input = view as? UITextField {
            let value = node["text"] as? String ?? ""
            let password = node["password"] as? Bool ?? false
            if input.isSecureTextEntry != password { input.isSecureTextEntry = password }
            if input.text != value && !(preserveComposition && input.markedTextRange != nil) { input.unmarkText(); input.text = value }
            input.placeholder = node["placeholder"] as? String
            input.isEnabled = node["enabled"] as? Bool ?? false
            input.textColor = color(style["foreground"])
        }
        if let input = view as? NativeTextView {
            let value = node["text"] as? String ?? ""
            if input.text != value && !(preserveComposition && input.markedTextRange != nil) { input.unmarkText(); input.text = value }
            input.placeholder.text = node["placeholder"] as? String
            input.isEditable = node["enabled"] as? Bool ?? false
            input.isSelectable = input.isEditable
            input.textColor = color(style["foreground"])
            input.setNeedsLayout()
        }
        if let checkbox = view as? NativeCheckbox {
            checkbox.toggle.isOn = node["checked"] as? Bool ?? false
            checkbox.toggle.isEnabled = node["enabled"] as? Bool ?? false
            checkbox.toggle.accessibilityLabel = node["name"] as? String ?? node["text"] as? String
            checkbox.caption.text = node["text"] as? String
            checkbox.caption.textColor = color(style["foreground"])
        }
    }

    func measure(_ json: String) throws -> CGSize {
        let request = try object(json)
        guard let node = request["node"] as? [String: Any], let width = request["maxWidth"] as? Double,
              let height = request["maxHeight"] as? Double else { throw NativeError.invalid("invalid native measure") }
        let view = try make(node)
        try apply(node, to: view)
        let fitting = view.sizeThatFits(CGSize(width: width, height: height))
        return CGSize(width: request["widthMode"] as? String == "exact" ? width : min(width, Double(fitting.width)),
                      height: request["heightMode"] as? String == "exact" ? height : min(height, Double(fitting.height)))
    }

    fileprivate func publish(_ json: String) throws {
        let snapshot = try object(json)
        guard snapshot["protocol"] as? Int == 1, let nodes = snapshot["nodes"] as? [[String: Any]],
              let token = snapshot["root"] as? String, root == 0 || token == String(root) else {
            throw NativeError.invalid("invalid native snapshot")
        }
        publishing = true
        defer { publishing = false }
        var live = Set<String>()
        var indices: [ObjectIdentifier: Int] = [:]
        for node in nodes {
            guard let id = node["id"] as? String, let frame = node["frame"] as? [Double], frame.count == 4 else {
                throw NativeError.invalid("invalid native node")
            }
            live.insert(id)
            let view = try views[id] ?? make(node)
            if let previous = descriptors[id], previous["kind"] as? String != node["kind"] as? String {
                throw NativeError.invalid("native node kind changed")
            }
            views[id] = view
            identities[ObjectIdentifier(view)] = id
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
                modal.dismissDialog = { [weak self, weak modal] backdrop in
                    guard let self = self, let modal = modal, !self.publishing,
                          let node = self.descriptors[id], !backdrop || node["closeOnOverlayClick"] as? Bool == true else { return }
                    let (revision, overflow) = self.revisions[id, default: 0].addingReportingOverflow(1)
                    guard !overflow else { return }
                    self.revisions[id] = revision
                    self.emit(modal, ["kind": "dismiss", "backdrop": backdrop, "revision": String(revision)])
                }
            }
            if let popover = view as? NativePopover {
                popover.acceptsDismiss = { [weak self, weak popover] in
                    guard let self = self, let popover = popover else { return false }
                    return !self.publishing && self.root != 0 && self.views[id] === popover
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
            }
            let parent: UIView
            var origin = [0.0, 0.0]
            if let parentID = node["parent"] as? String, let parentView = views[parentID],
               let parentFrame = descriptors[parentID]?["frame"] as? [Double] {
                parent = (parentView as? NativeModal)?.content ?? (parentView as? NativePopover)?.content ?? parentView; origin = [parentFrame[0], parentFrame[1]]
            } else if node["parent"] is NSNull { parent = self }
            else { throw NativeError.invalid("invalid native parent") }
            let index = indices[ObjectIdentifier(parent), default: 0]
            indices[ObjectIdentifier(parent)] = index + 1
            if view.superview !== parent || parent.subviews.firstIndex(of: view) != index { parent.insertSubview(view, at: index) }
            if descriptors[id].map({ NSDictionary(dictionary: $0).isEqual(NSDictionary(dictionary: node)) }) != true {
                let preserveComposition = descriptors[id]?["text"] as? String == node["text"] as? String && node["enabled"] as? Bool == true
                try apply(node, to: view, preserveComposition: preserveComposition)
            }
            view.frame = view is NativeModal || view is NativePopover ? .zero : CGRect(x: frame[0] - origin[0], y: frame[1] - origin[1], width: frame[2], height: frame[3])
            if let modal = view as? NativeModal, let style = node["style"] as? [String: Any] {
                modal.content.backgroundColor = color(style["background"])
            }
            if let popover = view as? NativePopover, let style = node["style"] as? [String: Any] {
                popover.content.backgroundColor = color(style["background"])
            }
            if let value = node["editRevision"] as? String, let revision = UInt64(value) { revisions[id] = max(revisions[id, default: 0], revision) }
            descriptors[id] = node
        }
        for id in popoverOrder.reversed() where !live.contains(id) { (views[id] as? NativePopover)?.retire() }
        popoverOrder.removeAll(where: { !live.contains($0) })
        for id in modalOrder.reversed() where !live.contains(id) { (views[id] as? NativeModal)?.hide() }
        for id in Array(views.keys) where !live.contains(id) {
            if let view = views.removeValue(forKey: id) {
                (view as? NativeSelect)?.retire()
                (view as? NativePopover)?.retire()
                (view as? NativeModal)?.hide()
                identities.removeValue(forKey: ObjectIdentifier(view)); view.removeFromSuperview()
            }
            descriptors.removeValue(forKey: id); revisions.removeValue(forKey: id)
        }
        for (id, view) in views {
            if descriptors[id]?["kind"] as? String == "scroll", let scroll = view as? UIScrollView {
                scroll.contentSize = CGSize(width: scroll.subviews.map { $0.frame.maxX }.max() ?? 0,
                                            height: scroll.subviews.map { $0.frame.maxY }.max() ?? 0)
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
                           name: descriptors[id]?["name"] as? String ?? "Dialog")
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
    }

    private func updatePopovers() throws {
        for id in popoverOrder {
            guard eventVisible(id), let popover = views[id] as? NativePopover, let node = descriptors[id],
                  let anchorID = node["anchor"] as? String, let anchor = views[anchorID] else { continue }
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
        publishing = true
        defer { publishing = false }
        for id in popoverOrder.reversed() { (views[id] as? NativePopover)?.retire() }
        for view in views.values { (view as? NativePopover)?.retire() }
        popoverOrder.removeAll()
        for id in modalOrder.reversed() { (views[id] as? NativeModal)?.hide() }
        for view in views.values { (view as? NativeSelect)?.retire() }
        modalOrder.removeAll()
        appliedContentSizeCategory = nil
        subviews.forEach { $0.removeFromSuperview() }
        views.removeAll(); descriptors.removeAll(); identities.removeAll(); revisions.removeAll()
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
        guard !publishing, root != 0, let id = identities[ObjectIdentifier(view)], eventVisible(id) else { return }
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
        let revision = revisions[id, default: 0] + 1
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
    public func textViewDidBeginEditing(_ textView: UITextView) {
        emit(textView, ["kind": "focus"])
        revealFocusedControl()
    }
    public func textViewDidChange(_ textView: UITextView) { multilineEdited(textView) }
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
    public func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard let id = identities[ObjectIdentifier(scrollView)], descriptors[id]?["kind"] as? String == "scroll" else { return }
        emit(scrollView, ["kind": "scroll", "x": scrollView.contentOffset.x, "y": scrollView.contentOffset.y])
        schedulePopovers()
    }
}

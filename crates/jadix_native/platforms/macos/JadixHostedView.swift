import AppKit
import WebKit
import AVKit
import AVFoundation
import MetalKit
import UniformTypeIdentifiers

/// A node-owned native surface. Dispose must release observers, timers and delegates.
@MainActor public protocol JadixEmbeddedView: AnyObject {
    var view: NSView { get }
    func update(_ properties: [String: Any], emit: @escaping (String, Any) -> Void)
    func dispose()
}
public typealias JadixEmbeddedViewFactory = @MainActor () -> any JadixEmbeddedView

// Every surface belongs to one presentation node. No process-global players, web
// delegates, script handlers, drawables or completion callbacks survive removal.
@MainActor final class JadixHostedView: NSView {
    var emit: ((String, Any) -> Void)?
    var documentChanged: ((Bool) -> Void)?
    var resourceAllowed: ((URL) -> Bool)?
    var resourceProvider: JadixWebResourceProvider?
    var embeddedFactory: ((String) -> JadixEmbeddedViewFactory?)?
    private(set) var documentGeneration = UUID()
    let content = JadixHostedContentView()
    private var specification: NSDictionary?
    private var surface: NSView?
    private var web: JadixHostedWeb?
    private var media: JadixHostedMedia?
    private var shader: JadixHostedShader?
    private var richEditor: JadixRichEditor?
    private var terminal: JadixTerminalView?
    private var embedded: (any JadixEmbeddedView)?
    private var embeddedKind: String?
    private var embeddedGeneration = UUID()
    private var actions: [JadixHostedAction] = []
    override var isFlipped: Bool { true }
    override var intrinsicContentSize: NSSize { surface?.intrinsicContentSize ?? NSSize(width: 320, height: 180) }
    override func layout() {
        super.layout(); content.frame = bounds; surface?.frame = bounds
        if specification?["type"] as? String == "menu_button", let button = surface as? NSButton {
            let authored = !content.subviews.isEmpty
            button.title = authored ? "" : specification?["label"] as? String ?? ""
            button.isBordered = !authored
        }
    }
    func update(_ value: [String: Any]) {
        let next = value as NSDictionary
        if specification == next { return }
        if specification?["type"] as? String != value["type"] as? String { dispose() }
        specification = next
        switch value["type"] as? String {
        case "terminal":
            if terminal == nil {
                let host = JadixTerminalView(frame: .zero); host.emit = { [weak self] event, data in self?.emit?(event, data) }; terminal = host; install(host)
            }
            terminal?.update(value); invalidateIntrinsicContentSize()
        case "rich_editor":
            if richEditor == nil {
                let editor = JadixRichEditor(frame: .zero)
                editor.isRichText = true; editor.isSelectable = true; editor.delegate = editor
                editor.emit = { [weak self] in self?.emit?($0, $1) }
                richEditor = editor; install(editor)
            }
            richEditor?.update(value); invalidateIntrinsicContentSize()
        case "custom":
            guard let kind = value["kind"] as? String, let properties = value["properties"] as? [String: Any] else { return }
            if embeddedKind != kind {
                embeddedGeneration = UUID(); embedded?.dispose(); embedded = nil; embeddedKind = kind
                surface?.removeFromSuperview(); surface = nil
            }
            if embedded == nil {
                guard let factory = embeddedFactory?(kind) else { emit?("error", "No native view factory registered for \(kind)"); return }
                let instance = factory()
                guard instance.view.superview == nil else { instance.dispose(); emit?("error", "Native view factory returned an attached view"); return }
                embedded = instance; install(instance.view)
            }
            let generation = embeddedGeneration
            embedded?.update(properties) { [weak self] event, data in
                guard let self, self.embeddedGeneration == generation else { return }; self.emit?(event, data)
            }
            invalidateIntrinsicContentSize()
        case "web":
            if web == nil {
                let host = JadixHostedWeb()
                host.resourceAllowed = { [weak self] in self?.resourceAllowed?($0) == true }
                host.resources.provider = { [weak self] request, reply in self?.resourceProvider?(request, reply) }
                host.emit = { [weak self] event, data in self?.emit?(event, data) }
                host.documentChanged = { [weak self] ready in
                    guard let self else { return }
                    if !ready { self.documentGeneration = UUID() }; self.documentChanged?(ready)
                }
                web = host; install(host.view)
            }
            web?.update(value)
        case "media":
            if media == nil {
                let host = JadixHostedMedia(); host.emit = { [weak self] event, data in self?.emit?(event, data) }; media = host; install(host.view)
            }
            media?.update(value)
        case "canvas":
            let canvas = surface as? JadixHostedCanvas ?? JadixHostedCanvas()
            install(canvas); canvas.commands = value["commands"] as? [[String: Any]] ?? []; canvas.needsDisplay = true
        case "shader":
            if shader == nil {
                let host = JadixHostedShader(); host.reportError = { [weak self] error in self?.emit?("error", error) }; shader = host; install(host.view)
            }
            shader?.update(value)
        case "menu_button":
            actions.removeAll()
            let button = surface as? NSButton ?? NSButton()
            button.title = value["label"] as? String ?? ""; button.setAccessibilityLabel(value["label"] as? String)
            button.target = self; button.action = #selector(showMenu(_:))
            button.bezelStyle = .rounded; button.isEnabled = value["enabled"] as? Bool == true
            install(button)
        case "segments", "tab_bar":
            actions.removeAll()
            let items = value["items"] as? [[String: Any]] ?? []
            let control = surface as? NSSegmentedControl ?? NSSegmentedControl()
            control.target = self; control.action = #selector(selectSegment(_:)); control.trackingMode = .selectOne
            control.isHidden = value["hidden"] as? Bool == true
            control.segmentCount = items.count; control.isEnabled = value["enabled"] as? Bool == true; control.selectedSegment = -1
            for (index, item) in items.enumerated() {
                let badge = (item["badge"] as? String).map { $0.isEmpty ? " •" : " (\($0))" } ?? ""
                control.setLabel((item["label"] as? String ?? "") + badge, forSegment: index)
                control.setImage((item["symbol"] as? String).flatMap { NSImage(systemSymbolName: $0, accessibilityDescription: item["accessibility_label"] as? String ?? item["label"] as? String) }, forSegment: index)
                control.setToolTip(item["accessibility_label"] as? String ?? item["label"] as? String, forSegment: index)
                control.setEnabled(item["enabled"] as? Bool == true, forSegment: index)
                if item["id"] as? String == value["selected"] as? String { control.selectedSegment = index }
            }
            install(control)
        case "date", "time":
            let control = surface as? NSDatePicker ?? NSDatePicker()
            control.target = self; control.action = #selector(selectDate(_:)); control.isEnabled = value["enabled"] as? Bool == true
            control.datePickerMode = .single; control.timeZone = TimeZone(secondsFromGMT: 0)
            var calendar = Calendar(identifier: .gregorian); calendar.timeZone = TimeZone(secondsFromGMT: 0)!
            control.calendar = calendar; control.locale = (value["locale"] as? String).map(Locale.init(identifier:)) ?? Locale.current
            let time = value["type"] as? String == "time"
            control.datePickerStyle = !time && value["mode"] as? String == "calendar" ? .clockAndCalendar : .textFieldAndStepper
            control.datePickerElements = time ? (value["seconds"] as? Bool == true ? [.hourMinuteSecond] : [.hourMinute]) : [.yearMonthDay]
            func date(_ object: Any?) -> Date? {
                guard let object = object as? [String: Any] else { return nil }
                var parts = DateComponents()
                parts.year = (object["year"] as? Int) ?? 2000; parts.month = (object["month"] as? Int) ?? 1; parts.day = (object["day"] as? Int) ?? 1
                parts.hour = (object["hour"] as? Int) ?? 12; parts.minute = (object["minute"] as? Int) ?? 0; parts.second = (object["second"] as? Int) ?? 0
                return calendar.date(from: parts)
            }
            control.minDate = date(value["minimum"]); control.maxDate = date(value["maximum"])
            if let selected = date(value["value"]) { control.dateValue = selected }; install(control)
        case "color":
            let control = surface as? JadixHostedColorWell ?? JadixHostedColorWell()
            control.target = self; control.action = #selector(selectColor(_:)); control.isEnabled = value["enabled"] as? Bool == true
            control.allowsAlpha = value["alpha"] as? Bool == true
            let rgba = value["rgba"] as? [NSNumber] ?? []
            if rgba.count == 4 { control.color = NSColor(srgbRed: CGFloat(rgba[0].doubleValue / 255), green: CGFloat(rgba[1].doubleValue / 255), blue: CGFloat(rgba[2].doubleValue / 255), alpha: control.allowsAlpha ? CGFloat(rgba[3].doubleValue / 255) : 1) }
            install(control)
        default: break
        }
    }
    private func install(_ view: NSView) {
        if surface === view { return }
        if content.superview == nil { content.setAccessibilityElement(false); addSubview(content) }
        surface?.removeFromSuperview(); surface = view; addSubview(view, positioned: .above, relativeTo: content)
        view.frame = bounds; content.frame = bounds
    }
    @objc private func showMenu(_ sender: NSButton) {
        guard let items = specification?["items"] as? [[String: Any]] else { return }; actions.removeAll()
        func build(_ items: [[String: Any]], enabled: Bool) -> NSMenu {
            let menu = NSMenu(); menu.autoenablesItems = false
            for item in items {
                let available = enabled && item["enabled"] as? Bool == true
                if item["inline"] as? Bool == true {
                    let section = build(item["children"] as? [[String: Any]] ?? [], enabled: available)
                    if !menu.items.isEmpty { menu.addItem(.separator()) }
                    for child in section.items { section.removeItem(child); menu.addItem(child) }
                    menu.addItem(.separator()); continue
                }
                let entry = NSMenuItem(title: item["label"] as? String ?? "", action: nil, keyEquivalent: "")
                entry.isEnabled = available; entry.state = item["checked"] as? Bool == true ? .on : .off
                entry.image = (item["symbol"] as? String).flatMap { NSImage(systemSymbolName: $0, accessibilityDescription: nil) }
                entry.setAccessibilityLabel(item["accessibility_label"] as? String ?? entry.title)
                if item["destructive"] as? Bool == true { entry.attributedTitle = NSAttributedString(string: entry.title, attributes: [.foregroundColor: NSColor.systemRed]) }
                let shortcut = JadixMenuShortcut(item); entry.keyEquivalent = entry.isEnabled ? shortcut.key : ""
                var modifiers: NSEvent.ModifierFlags = []
                if shortcut.command { modifiers.insert(.command) }; if shortcut.control { modifiers.insert(.control) }
                if shortcut.alt { modifiers.insert(.option) }; if shortcut.shift { modifiers.insert(.shift) }
                entry.keyEquivalentModifierMask = modifiers
                let children = item["children"] as? [[String: Any]] ?? []
                if !children.isEmpty { entry.submenu = build(children, enabled: entry.isEnabled) }
                else {
                    let action = JadixHostedAction { [weak self] in self?.emit?("select", item["id"] as? String ?? "") }
                    actions.append(action); entry.target = action; entry.action = #selector(JadixHostedAction.invoke)
                }
                menu.addItem(entry)
            }
            return menu
        }
        build(items, enabled: sender.isEnabled).popUp(positioning: nil, at: NSPoint(x: 0, y: sender.bounds.height), in: sender)
    }
    @objc private func selectSegment(_ sender: NSSegmentedControl) {
        guard let items = specification?["items"] as? [[String: Any]], items.indices.contains(sender.selectedSegment),
              let id = items[sender.selectedSegment]["id"] as? String else { return }; emit?("select", id)
    }
    @objc private func selectDate(_ sender: NSDatePicker) {
        var calendar = Calendar(identifier: .gregorian); calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        let parts = calendar.dateComponents([.year, .month, .day, .hour, .minute, .second], from: sender.dateValue)
        if specification?["type"] as? String == "time" { emit?("change", ["hour": parts.hour ?? 0, "minute": parts.minute ?? 0, "second": specification?["seconds"] as? Bool == true ? parts.second ?? 0 : 0]) }
        else { emit?("change", ["year": parts.year ?? 2000, "month": parts.month ?? 1, "day": parts.day ?? 1]) }
    }
    @objc private func selectColor(_ sender: JadixHostedColorWell) {
        guard let color = sender.color.usingColorSpace(.sRGB) else { return }
        func channel(_ value: CGFloat) -> Int { Int((min(1, max(0, value)) * 255).rounded()) }
        emit?("change", [channel(color.redComponent), channel(color.greenComponent), channel(color.blueComponent), sender.allowsAlpha ? channel(color.alphaComponent) : 255])
    }
    func evaluate(_ script: String, completion: @escaping (Any?, Error?) -> Void) {
        guard let web else { completion(nil, NSError(domain: "JadixWeb", code: 1)); return }; web.view.evaluateJavaScript(script, completionHandler: completion)
    }
    func dispose() {
        documentGeneration = UUID(); embeddedGeneration = UUID()
        embedded?.dispose(); embedded = nil; embeddedKind = nil
        web?.dispose(); media?.dispose(); shader?.dispose(); richEditor?.dispose(); terminal?.dispose()
        web = nil; media = nil; shader = nil; richEditor = nil; terminal = nil
        actions.removeAll(); surface?.removeFromSuperview(); surface = nil; specification = nil
    }
}
@MainActor final class JadixHostedContentView: NSView {
    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func accessibilityChildren() -> [Any]? { [] }
}
@MainActor private final class JadixHostedAction: NSObject {
    let callback: () -> Void
    init(_ callback: @escaping () -> Void) { self.callback = callback }
    @objc func invoke() { callback() }
}
@MainActor private final class JadixHostedColorWell: NSColorWell {
    var allowsAlpha = true
    override func activate(_ exclusive: Bool) { NSColorPanel.shared.showsAlpha = allowsAlpha; super.activate(exclusive) }
}
@MainActor private final class JadixHostedCanvas: NSView {
    var commands: [[String: Any]] = []
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        for command in commands {
            let rgba = command["color"] as? [Double] ?? [0, 0, 0, 255]
            guard rgba.count == 4 else { continue }
            let color = NSColor(srgbRed: rgba[0] / 255, green: rgba[1] / 255, blue: rgba[2] / 255, alpha: rgba[3] / 255)
            switch command["type"] as? String {
            case "fill":
                if let r = command["rect"] as? [Double], r.count == 4 { context.setFillColor(color.cgColor); context.fill(CGRect(x: r[0], y: r[1], width: r[2], height: r[3])) }
            case "stroke":
                guard let points = command["points"] as? [[Double]], let first = points.first, first.count == 2 else { continue }
                context.beginPath(); context.move(to: CGPoint(x: first[0], y: first[1]))
                for point in points.dropFirst() where point.count == 2 { context.addLine(to: CGPoint(x: point[0], y: point[1])) }
                if command["closed"] as? Bool == true { context.closePath() }
                context.setStrokeColor(color.cgColor); context.setLineWidth(CGFloat(command["width"] as? Double ?? 1)); context.strokePath()
            case "text":
                if let text = command["text"] as? String, let p = command["position"] as? [Double], p.count == 2 {
                    (text as NSString).draw(at: NSPoint(x: p[0], y: p[1]), withAttributes: [.font: NSFont.systemFont(ofSize: CGFloat(command["size"] as? Double ?? 14)), .foregroundColor: color])
                }
            default: break
            }
        }
    }
}

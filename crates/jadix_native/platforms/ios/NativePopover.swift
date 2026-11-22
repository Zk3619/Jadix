import UIKit

private final class NativePopoverSurface: UIView {
    let content = UIView()
    var dismiss: (() -> Void)?
    var moveFocus: ((Bool) -> Bool)?
    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(content)
        isAccessibilityElement = false
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    override var canBecomeFirstResponder: Bool { true }
    override var keyCommands: [UIKeyCommand]? {
        [UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(escape)),
         UIKeyCommand(input: "\t", modifierFlags: [], action: #selector(tab)),
         UIKeyCommand(input: "\t", modifierFlags: [.shift], action: #selector(tab))]
    }
    private var composing: Bool {
        func marked(_ view: UIView) -> Bool {
            if view.isFirstResponder, let input = view as? UITextInput, input.markedTextRange != nil { return true }
            return view.subviews.contains(where: marked)
        }
        return window.map(marked) ?? false
    }
    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(tab(_:)) || action == #selector(escape) { return !composing }
        return super.canPerformAction(action, withSender: sender)
    }
    @objc private func tab(_ command: UIKeyCommand) { if !composing { _ = moveFocus?(command.modifierFlags.contains(.shift)) } }
    @objc private func escape() { if !composing { dismiss?() } }
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard !isHidden, alpha > 0.01, isUserInteractionEnabled, content.frame.contains(point) else { return nil }
        return content.hitTest(convert(point, to: content), with: event)
    }
}

final class NativePopover: UIView, UIGestureRecognizerDelegate {
    private let surface = NativePopoverSurface(frame: .zero)
    var content: UIView { surface.content }
    var dismissPopover: ((Bool) -> Void)?
    var cancelPopover: ((Bool) -> Void)?
    var acceptsDismiss: (() -> Bool)?
    var moveFocus: ((Bool) -> Bool)?
    var notify: ((JadixPopoverEvent) -> Void)?
    private lazy var visuals = JadixViewAppearance(content)
    private var appearanceWire: [String: Any] = [:]
    private var visibleAppearance = JadixVisualStyle()
    private var endingAppearance = JadixPopoverAppearance()
    private var exitDuration: TimeInterval = 0
    private var closeOnEscape = true
    private var closeOnOutsidePress = true
    private var closing = false, entering = false
    private var animationRevision: UInt64 = 0
    private(set) var reportedOpen = false
    private var position: JadixPopoverEvent.Position?
    private weak var sourceWindow: UIWindow?
    private weak var anchor: UIView?
    private weak var previousFocus: UIView?
    private var outsideGesture: UITapGestureRecognizer?
    private var restoresFocus = true

    override init(frame: CGRect) {
        super.init(frame: frame)
        isAccessibilityElement = false
        surface.dismiss = { [weak self] in self?.requestDismiss(backdrop: false) }
        surface.moveFocus = { [weak self] in self?.moveFocus?($0) == true }
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    var isPresented: Bool { surface.superview != nil && !closing }

    private func report(_ event: JadixPopoverEvent) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            switch event {
            case .opened: guard self.isPresented, !self.reportedOpen else { return }; self.reportedOpen = true
            case .closed: guard self.surface.superview == nil, self.reportedOpen else { return }; self.reportedOpen = false
            case .positionChanged(let value): guard self.position == value, !self.closing else { return }
            case .dismissRequested: break
            }
            self.notify?(event)
        }
    }
    private func reportPosition(_ value: JadixPopoverEvent.Position?) {
        guard value != position else { return }; position = value; report(.positionChanged(value))
    }
    func suspend() {
        animationRevision &+= 1; visuals.cancel(); entering = false
        surface.isHidden = true; surface.endEditing(true); reportPosition(nil)
    }

    private func focused(_ view: UIView) -> UIView? {
        if view.isFirstResponder { return view }
        for child in view.subviews { if let focused = focused(child) { return focused } }
        return nil
    }

    func show(in host: UIView, anchor: UIView, node: [String: Any]) throws {
        guard let window = anchor.window, host.window != nil else { suspend(); return }
        var ancestor: UIView? = anchor
        while let view = ancestor { if view.isHidden || view.alpha <= 0.01 { suspend(); return }; ancestor = view.superview }
        if sourceWindow !== window { hide(restore: false, animated: false) }
        let fresh = !isPresented || surface.isHidden
        self.anchor = anchor
        if surface.superview == nil {
            sourceWindow = window
            previousFocus = focused(window)
            surface.frame = window.bounds
            surface.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            window.addSubview(surface)
        }
        if outsideGesture == nil {
            let gesture = UITapGestureRecognizer(target: self, action: #selector(outside(_:)))
            gesture.cancelsTouchesInView = false
            gesture.delaysTouchesEnded = false
            gesture.delegate = self
            window.addGestureRecognizer(gesture)
            outsideGesture = gesture
            restoresFocus = true
        }
        let style = node["style"] as? [String: Any] ?? [:]
        appearanceWire = style["appearance"] as? [String: Any] ?? [:]
        visibleAppearance = JadixVisualStyle(appearanceWire, background: JadixVisualStyle.color(style["background"]) ?? .clear)
        endingAppearance = JadixPopoverAppearance(style: style["popover_ending"] as? [String: Any])
        exitDuration = (style["popover_exit_ms"] as? Double ?? 0) / 1000
        closeOnEscape = style["popover_close_on_escape"] as? Bool ?? true
        closeOnOutsidePress = node["closeOnOutsideClick"] as? Bool == true
        surface.isHidden = false; surface.isUserInteractionEnabled = true; surface.accessibilityElementsHidden = false
        closing = false
        try position(in: host, anchor: anchor, node: node)
        guard !surface.isHidden else { return }
        if fresh {
            let from = entering || visuals.isAnimating ? visuals.current : JadixPopoverAppearance(style: style["popover_starting"] as? [String: Any]).resolved(over: visibleAppearance)
            animationRevision &+= 1; let revision = animationRevision; entering = true
            visuals.animate(from: from, to: visibleAppearance, duration: (style["popover_enter_ms"] as? Double ?? 0) / 1000) { [weak self] in
                guard let self, self.animationRevision == revision, !self.closing else { return }
                self.entering = false; self.visuals.update(self.appearanceWire, background: self.visibleAppearance.background)
            }
            if !jadixFocusView(content) { _ = surface.becomeFirstResponder() }
            UIAccessibility.post(notification: .layoutChanged, argument: content)
            report(.opened)
        } else if !entering { visuals.update(appearanceWire, background: visibleAppearance.background) }
        else { visuals.render(visuals.current) }
    }

    func position(in host: UIView, anchor: UIView, node: [String: Any]) throws {
        guard let window = sourceWindow, isPresented, anchor.window === window,
              let frame = node["frame"] as? [Double], frame.count == 4,
              let placement = node["placement"] as? String, let gap = node["gap"] as? Double else { return }
        let viewport = host.convert(host.bounds, to: window).intersection(window.safeAreaLayoutGuide.layoutFrame)
        guard !viewport.isNull, !viewport.isEmpty else { suspend(); return }
        surface.frame = window.bounds
        let style = node["style"] as? [String: Any]
        let inset = min(CGFloat(style?["popover_margin"] as? Double ?? 0), min(viewport.width, viewport.height) / 2)
        let width = min(CGFloat(frame[2]), max(0, viewport.width - inset * 2))
        let height = min(CGFloat(frame[3]), max(0, viewport.height - inset * 2))
        let target = anchor.convert(anchor.bounds, to: window)
        // Intersect every ancestor clip, including a scrolled-away anchor.
        var visibleAnchor = target, ancestor = anchor.superview
        while let view = ancestor {
            if view.clipsToBounds { visibleAnchor = visibleAnchor.intersection(view.convert(view.bounds, to: window)) }
            ancestor = view.superview
        }
        guard !visibleAnchor.intersection(viewport).isEmpty, !visibleAnchor.isNull else { suspend(); return }
        let parts = placement.split(separator: "-")
        guard let side = parts.first else { throw NSError(domain: "JadixNativePopover", code: 1) }
        var alignment = parts.count > 1 ? String(parts[1]) : "center"
        let vertical = side == "top" || side == "bottom"
        if vertical && anchor.effectiveUserInterfaceLayoutDirection == .rightToLeft { alignment = alignment == "start" ? "end" : alignment == "end" ? "start" : alignment }
        var before = side == "top" || side == "left"
        let start = vertical ? target.minY : target.minX
        let extent = vertical ? target.height : target.width
        let lower = vertical ? viewport.minY : viewport.minX
        let upper = vertical ? viewport.maxY : viewport.maxX
        let primarySize = vertical ? height : width
        let beforeRoom = start - lower - inset - CGFloat(gap)
        let afterRoom = upper - start - extent - inset - CGFloat(gap)
        if (before ? beforeRoom : afterRoom) < primarySize && (before ? afterRoom > beforeRoom : beforeRoom > afterRoom) { before.toggle() }
        let primary = before ? start - CGFloat(gap) - primarySize : start + extent + CGFloat(gap)
        let crossStart = vertical ? target.minX : target.minY
        let crossExtent = vertical ? target.width : target.height
        let crossSize = vertical ? width : height
        let cross = alignment == "start" ? crossStart : alignment == "end" ? crossStart + crossExtent - crossSize : crossStart + (crossExtent - crossSize) / 2
        let left = max(viewport.minX + inset, min(vertical ? cross : primary, viewport.maxX - inset - width))
        let top = max(viewport.minY + inset, min(vertical ? primary : cross, viewport.maxY - inset - height))
        let positionedFrame = CGRect(x: left, y: top, width: width, height: height)
        // UIView.frame is undefined while the appearance renderer applies a
        // transform. Keep layout geometry independent of animated scale/offset.
        content.bounds = CGRect(origin: .zero, size: positionedFrame.size)
        content.center = CGPoint(x: positionedFrame.midX, y: positionedFrame.midY)
        content.accessibilityLabel = node["name"] as? String
        window.bringSubviewToFront(surface)
        let effectiveSide = vertical ? (before ? "top" : "bottom") : (before ? "left" : "right")
        let available = max(0, before ? beforeRoom : afterRoom)
        reportPosition(.init(frame: host.convert(positionedFrame, from: surface), placement: effectiveSide + (parts.count > 1 ? "-" + String(parts[1]) : ""), availableSize: vertical ? CGSize(width: viewport.width - inset * 2, height: available) : CGSize(width: available, height: viewport.height - inset * 2)))
    }

    func requestDismiss(backdrop: Bool) {
        guard isPresented, acceptsDismiss?() == true else { return }
        let allowed = backdrop ? closeOnOutsidePress : closeOnEscape
        report(.dismissRequested(backdrop ? .outsidePress : .escape, allowed: allowed))
        cancelPopover?(backdrop)
        // A cancellation callback may replace the node or close the surface.
        guard allowed, isPresented, acceptsDismiss?() == true else { return }
        restoresFocus = !backdrop
        dismissPopover?(backdrop)
    }

    @objc private func outside(_ gesture: UITapGestureRecognizer) {
        if gesture.state == .ended { requestDismiss(backdrop: true) }
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard acceptsDismiss?() == true, let view = touch.view else { return false }
        return !view.isDescendant(of: content)
    }

    func hide(restore: Bool, animated: Bool = true) {
        guard surface.superview != nil, !closing || !animated else { return }
        let presented = isPresented
        let hadFocus = focused(surface) != nil
        if let gesture = outsideGesture { gesture.view?.removeGestureRecognizer(gesture) }
        outsideGesture = nil
        surface.endEditing(true)
        surface.isUserInteractionEnabled = false; surface.accessibilityElementsHidden = true
        if presented && restore && restoresFocus && hadFocus {
            let restored = anchor?.window != nil && anchor?.becomeFirstResponder() == true
            if !restored, previousFocus?.window != nil { _ = previousFocus?.becomeFirstResponder() }
            UIAccessibility.post(notification: .layoutChanged, argument: anchor ?? previousFocus)
        }
        previousFocus = nil
        anchor = nil
        restoresFocus = true
        animationRevision &+= 1; let revision = animationRevision
        closing = true; entering = false; position = nil
        let finish = { [weak self] in
            guard let self, self.animationRevision == revision, self.closing else { return }
            self.visuals.cancel(); self.surface.removeFromSuperview(); self.sourceWindow = nil; self.closing = false; self.report(.closed)
        }
        if animated && !surface.isHidden { visuals.animate(to: endingAppearance.resolved(over: visibleAppearance), duration: exitDuration, completion: finish) }
        else { finish() }
    }

    func retire() {
        if closing { animationRevision &+= 1; visuals.cancel(); closing = false; surface.removeFromSuperview(); sourceWindow = nil }
        hide(restore: false, animated: false)
        visuals.remove()
        dismissPopover = nil
        cancelPopover = nil
        acceptsDismiss = nil
        moveFocus = nil
    }
}

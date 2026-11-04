import UIKit

private final class NativePopoverSurface: UIView {
    let content = UIView()
    var dismiss: (() -> Void)?
    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(content)
        content.clipsToBounds = true
        isAccessibilityElement = false
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    override var canBecomeFirstResponder: Bool { true }
    override var keyCommands: [UIKeyCommand]? {
        [UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(escape))]
    }
    @objc private func escape() { dismiss?() }
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard !isHidden, alpha > 0.01, isUserInteractionEnabled, content.frame.contains(point) else { return nil }
        return content.hitTest(convert(point, to: content), with: event)
    }
}

final class NativePopover: UIView, UIGestureRecognizerDelegate {
    private let surface = NativePopoverSurface(frame: .zero)
    var content: UIView { surface.content }
    var dismissPopover: ((Bool) -> Void)?
    var acceptsDismiss: (() -> Bool)?
    private weak var sourceWindow: UIWindow?
    private weak var anchor: UIView?
    private weak var previousFocus: UIView?
    private var outsideGesture: UITapGestureRecognizer?
    private var restoresFocus = true

    override init(frame: CGRect) {
        super.init(frame: frame)
        isAccessibilityElement = false
        surface.dismiss = { [weak self] in self?.requestDismiss(backdrop: false) }
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    var isPresented: Bool { surface.superview != nil }

    private func focused(_ view: UIView) -> UIView? {
        if view.isFirstResponder { return view }
        for child in view.subviews { if let focused = focused(child) { return focused } }
        return nil
    }

    private func focusFirst(_ view: UIView) -> Bool {
        if view is UITextField || view is UITextView || view is UIControl {
            if !view.isHidden, view.isUserInteractionEnabled, (view as? UIControl)?.isEnabled != false, view.becomeFirstResponder() { return true }
        }
        for child in view.subviews where !child.isHidden { if focusFirst(child) { return true } }
        return false
    }

    func show(in host: UIView, anchor: UIView, node: [String: Any]) throws {
        guard let window = anchor.window, host.window != nil else { return }
        if sourceWindow !== window { hide(restore: false) }
        let fresh = !isPresented
        self.anchor = anchor
        if fresh {
            sourceWindow = window
            previousFocus = focused(window)
            surface.frame = window.bounds
            surface.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            window.addSubview(surface)
            let gesture = UITapGestureRecognizer(target: self, action: #selector(outside(_:)))
            gesture.cancelsTouchesInView = false
            gesture.delaysTouchesEnded = false
            gesture.delegate = self
            window.addGestureRecognizer(gesture)
            outsideGesture = gesture
            restoresFocus = true
        }
        try position(in: host, anchor: anchor, node: node)
        if fresh && isPresented {
            if !focusFirst(content) { _ = surface.becomeFirstResponder() }
            UIAccessibility.post(notification: .layoutChanged, argument: content)
        }
    }

    func position(in host: UIView, anchor: UIView, node: [String: Any]) throws {
        guard let window = sourceWindow, isPresented, anchor.window === window,
              let frame = node["frame"] as? [Double], frame.count == 4,
              let placement = node["placement"] as? String, let gap = node["gap"] as? Double else { return }
        let viewport = host.convert(host.bounds, to: window).intersection(window.safeAreaLayoutGuide.layoutFrame)
        guard !viewport.isNull, !viewport.isEmpty else { hide(restore: false); return }
        surface.frame = window.bounds
        let inset: CGFloat = 8
        let width = min(CGFloat(frame[2]), max(0, viewport.width - inset * 2))
        let height = min(CGFloat(frame[3]), max(0, viewport.height - inset * 2))
        let target = anchor.convert(anchor.bounds, to: window)
        let parts = placement.split(separator: "-")
        guard let side = parts.first else { throw NSError(domain: "JadixNativePopover", code: 1) }
        let alignment = parts.count > 1 ? String(parts[1]) : "center"
        let vertical = side == "top" || side == "bottom"
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
        content.frame = CGRect(x: left, y: top, width: width, height: height)
        content.accessibilityLabel = node["name"] as? String
        window.bringSubviewToFront(surface)
    }

    func requestDismiss(backdrop: Bool) {
        guard isPresented, acceptsDismiss?() == true else { return }
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

    func hide(restore: Bool) {
        let presented = isPresented
        let hadFocus = focused(surface) != nil
        if let gesture = outsideGesture { gesture.view?.removeGestureRecognizer(gesture) }
        outsideGesture = nil
        surface.endEditing(true)
        surface.removeFromSuperview()
        sourceWindow = nil
        if presented && restore && restoresFocus && hadFocus {
            let restored = anchor?.window != nil && anchor?.becomeFirstResponder() == true
            if !restored, previousFocus?.window != nil { _ = previousFocus?.becomeFirstResponder() }
            UIAccessibility.post(notification: .layoutChanged, argument: anchor ?? previousFocus)
        }
        previousFocus = nil
        anchor = nil
        restoresFocus = true
    }

    func retire() {
        hide(restore: false)
        dismissPopover = nil
        acceptsDismiss = nil
    }
}

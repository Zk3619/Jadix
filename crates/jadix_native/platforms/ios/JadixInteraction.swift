import UIKit
import UIKit.UIGestureRecognizerSubclass

/// One adapter for the shared interaction protocol. It never installs on native
/// editors/hosted surfaces. Raw touch observation does not participate in the
/// gesture arena; pan/pinch/rotation cooperate on the same target.
@MainActor final class JadixInteractionInput: NSObject, UIGestureRecognizerDelegate {
    private weak var view: UIView?
    private var policy: [String: Any] = [:]
    private var signature = NSDictionary()
    private var recognizers: [UIGestureRecognizer] = []
    private var transforms: [ObjectIdentifier: (kind: String, position: CGPoint, fields: [String: Any])] = [:]
    private var callback: (([String: Any]) -> Void)?
    private var legacyHover = false
    private var hovered = false
    private var generation: UInt64 = 0

    func configure(_ value: [String: Any], view: UIView, button: Bool, emit: @escaping ([String: Any]) -> Void) {
        callback = emit
        guard self.view !== view || !signature.isEqual(NSDictionary(dictionary: value)) || legacyHover != button else { return }
        dispose()
        self.view = view; policy = value; signature = NSDictionary(dictionary: value); legacyHover = button; callback = emit
        func install(_ recognizer: UIGestureRecognizer) {
            recognizer.delegate = self; view.addGestureRecognizer(recognizer); recognizers.append(recognizer)
        }
        if enabled("press") || value["longPressMillis"] is NSNumber { install(JadixPressRecognizer(owner: self)) }
        if enabled("touch") { install(JadixTouchRecognizer(owner: self)) }
        if value["pan"] is String { install(JadixPanRecognizer(owner: self, target: self, action: #selector(gesture(_:)))) }
        if enabled("pinch") { install(UIPinchGestureRecognizer(target: self, action: #selector(gesture(_:)))) }
        if enabled("rotation") { install(UIRotationGestureRecognizer(target: self, action: #selector(gesture(_:)))) }
        if enabled("hover") || button { install(UIHoverGestureRecognizer(target: self, action: #selector(gesture(_:)))) }
        if !recognizers.isEmpty { view.isUserInteractionEnabled = true }
    }
    fileprivate func enabled(_ key: String) -> Bool { policy[key] as? Bool == true }
    fileprivate var longPressDelay: Double? { (policy["longPressMillis"] as? NSNumber).map { $0.doubleValue / 1000 } }
    fileprivate var active: Bool {
        guard let view, view.window?.isKeyWindow == true,
              view.window?.windowScene?.activationState == .foregroundActive,
              UIApplication.shared.applicationState == .active else { return false }
        var ancestor: UIView? = view
        while let item = ancestor {
            if item.isHidden || item.alpha <= 0.01 || !item.isUserInteractionEnabled || (item as? UIControl)?.isEnabled == false { return false }
            ancestor = item.superview
        }
        return true
    }
    fileprivate func send(_ kind: String, _ phase: String, position: CGPoint, timestamp: Double = ProcessInfo.processInfo.systemUptime,
                          modifiers: UIKeyModifierFlags = [], fields: [String: Any] = [:]) {
        guard (phase == "cancelled" || active), position.x.isFinite, position.y.isFinite,
              abs(position.x) <= 1e6, abs(position.y) <= 1e6 else { return }
        var event: [String: Any] = ["kind": kind, "phase": phase, "position": [position.x, position.y], "timestamp": timestamp,
            "modifiers": ["shift": modifiers.contains(.shift), "control": modifiers.contains(.control),
                          "alt": modifiers.contains(.alternate), "meta": modifiers.contains(.command), "capsLock": modifiers.contains(.alphaShift)]]
        event.merge(fields) { _, new in new }
        callback?(["kind": "interaction", "event": event])
    }
    func cancel() {
        // Copy because an event callback can synchronously republish/dispose.
        let previous = recognizers, pending = Array(transforms.values), token = generation
        transforms.removeAll()
        if hovered { hovered = false; callback?(["kind": "hover", "inside": false]) }
        for event in pending {
            guard generation == token else { return }
            send(event.kind, "cancelled", position: event.position, fields: event.fields)
        }
        for recognizer in previous {
            guard generation == token else { return }
            (recognizer as? JadixPressRecognizer)?.cancelPress()
            (recognizer as? JadixTouchRecognizer)?.cancelTouches()
            recognizer.isEnabled = false
            if recognizers.contains(where: { $0 === recognizer }) { recognizer.isEnabled = true }
        }
        transforms.removeAll()
    }
    func dispose() {
        generation &+= 1; hovered = false
        callback = nil
        for recognizer in recognizers { recognizer.isEnabled = false; recognizer.view?.removeGestureRecognizer(recognizer) }
        recognizers.removeAll(); transforms.removeAll(); view = nil; policy = [:]; signature = NSDictionary(); legacyHover = false
    }
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard active, let view else { return false }
        // Passive raw observation includes descendants and native controls;
        // it cannot prevent their recognizers or consume their touch delivery.
        if recognizer is JadixTouchRecognizer { return touch.view?.isDescendant(of: view) == true }
        var target = touch.view
        while let item = target, item !== view {
            // A child control owns its native input. A deeper subscription owns
            // the same gesture family rather than delivering duplicate events.
            if item is UIControl || item is UITextView || item is JadixHostedView { return false }
            for nested in item.gestureRecognizers ?? [] {
                if nested.delegate is JadixInteractionInput && type(of: nested) == type(of: recognizer) { return false }
            }
            target = item.superview
        }
        return target === view
    }
    func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool { active }
    fileprivate func panDecision(_ delta: CGPoint) -> Int8 {
        let axis: UInt8 = policy["pan"] as? String == "horizontal" ? 1 : policy["pan"] as? String == "vertical" ? 2 : 0
        let limits = policy["panActivation"] as? [String: Any] ?? [:]
        func limit(_ key: String) -> Double { (limits[key] as? NSNumber)?.doubleValue ?? -1 }
        return jadix_native_pan_decision(axis, Double(delta.x), Double(delta.y),
            limit("activeOffsetX"), limit("activeOffsetY"), limit("failOffsetX"), limit("failOffsetY"))
    }
    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
        guard recognizer is JadixPanRecognizer, let view,
              let scroll = other.view as? UIScrollView, other === scroll.panGestureRecognizer else { return false }
        // An inner directional pan decides first. A cross-axis move fails it
        // through the shared threshold policy and releases outer scrolling.
        return view !== scroll && view.isDescendant(of: scroll)
    }
    func gestureRecognizer(_ first: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith second: UIGestureRecognizer) -> Bool {
        if first is JadixTouchRecognizer || second is JadixTouchRecognizer { return true }
        func transform(_ value: UIGestureRecognizer) -> Bool { value is JadixPanRecognizer || value is UIPinchGestureRecognizer || value is UIRotationGestureRecognizer }
        return first.view === second.view && transform(first) && transform(second)
    }
    @objc private func gesture(_ recognizer: UIGestureRecognizer) {
        guard let view, recognizers.contains(where: { $0 === recognizer }) else { return }
        let token = generation
        let phase: String
        switch recognizer.state {
        case .began: phase = "started"
        case .changed: phase = "changed"
        case .ended: phase = "ended"
        case .cancelled, .failed: phase = "cancelled"
        default: return
        }
        let position = recognizer.location(in: view.window)
        var kind = "", fields: [String: Any] = [:]
        if let pan = recognizer as? JadixPanRecognizer {
            kind = "pan"; let translation = pan.translation, velocity = pan.velocity
            fields = ["translation": [translation.x, translation.y], "velocity": [velocity.x, velocity.y]]
        } else if let pinch = recognizer as? UIPinchGestureRecognizer {
            kind = "pinch"; fields = ["value": pinch.scale, "rate": pinch.velocity]
        } else if let rotation = recognizer as? UIRotationGestureRecognizer {
            kind = "rotation"; fields = ["value": rotation.rotation, "rate": rotation.velocity]
        } else if recognizer is UIHoverGestureRecognizer {
            kind = "hover"
            if legacyHover {
                if phase != "changed" { hovered = phase == "started"; callback?(["kind": "hover", "inside": hovered]) }
                guard generation == token else { return }
                if phase == "started" || phase == "changed" { callback?(["kind": "pointerMove", "x": position.x, "y": position.y]) }
                guard generation == token else { return }
            }
            guard enabled("hover") else { return }
        }
        guard !kind.isEmpty else { return }
        let key = ObjectIdentifier(recognizer)
        if phase != "started" && transforms[key] == nil { return }
        transforms[key] = (kind, position, fields)
        if phase == "ended" || phase == "cancelled" { transforms.removeValue(forKey: key) }
        send(kind, phase, position: position, modifiers: recognizer.modifierFlags, fields: fields)
    }
}

/// Holds the native scroll arena while the shared gate waits. UIPan's fixed
/// recognition threshold cannot express a larger authored activation distance.
@MainActor private final class JadixPanRecognizer: UIGestureRecognizer {
    private weak var owner: JadixInteractionInput?
    private weak var finger: UITouch?
    private var origin = CGPoint.zero, previous = CGPoint.zero
    private var previousTime: TimeInterval = 0
    private(set) var translation = CGPoint.zero, velocity = CGPoint.zero
    init(owner: JadixInteractionInput, target: Any?, action: Selector?) {
        self.owner = owner; super.init(target: target, action: action)
        cancelsTouchesInView = true; delaysTouchesBegan = false; delaysTouchesEnded = false
    }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard finger == nil, let touch = touches.first else { return }
        finger = touch; origin = touch.location(in: view?.window); previous = origin; previousTime = touch.timestamp
    }
    private func sample(_ touch: UITouch) {
        let at = touch.location(in: view?.window), elapsed = touch.timestamp - previousTime
        translation = CGPoint(x: at.x - origin.x, y: at.y - origin.y)
        velocity = elapsed > 0 ? CGPoint(x: (at.x - previous.x) / elapsed, y: (at.y - previous.y) / elapsed) : .zero
        previous = at; previousTime = touch.timestamp
    }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let finger, touches.contains(finger) else { return }
        sample(finger)
        if state == .possible {
            switch owner?.panDecision(translation) ?? -1 {
            case 1: state = .began
            case 0: break
            default: state = .failed
            }
        } else if state == .began || state == .changed { state = .changed }
    }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let finger, touches.contains(finger) else { return }
        guard state == .possible || state == .began || state == .changed else { return }
        sample(finger); state = state == .possible ? .failed : .ended
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { state = .cancelled }
    override func reset() {
        finger = nil; translation = .zero; velocity = .zero; previousTime = 0; super.reset()
    }
}

/// Possible until release so a scroll/pan can win without delayed touch replay.
@MainActor private final class JadixPressRecognizer: UIGestureRecognizer {
    private weak var owner: JadixInteractionInput?
    private weak var tracked: UITouch?
    private var pressed = false, longPressed = false
    private var generation: UInt64 = 0
    init(owner: JadixInteractionInput) { self.owner = owner; super.init(target: nil, action: nil); cancelsTouchesInView = false }
    private func position(_ touch: UITouch) -> CGPoint { touch.location(in: view?.window) }
    private func down(_ touch: UITouch) {
        guard !pressed, !longPressed, tracked === touch, owner?.active == true else { return }
        pressed = true
        if owner?.enabled("press") == true { owner?.send("press", "started", position: position(touch), timestamp: touch.timestamp) }
    }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard tracked == nil, touches.count == 1, let touch = touches.first else { cancelPress(); state = .failed; return }
        tracked = touch; generation &+= 1; let token = generation
        var ancestor = view?.superview, scrolling = false
        while let item = ancestor { if item is UIScrollView { scrolling = true }; ancestor = item.superview }
        if scrolling {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.13) { [weak self, weak touch] in
                guard let self, let touch, self.generation == token else { return }; self.down(touch)
            }
        } else { down(touch) }
        if let delay = owner?.longPressDelay {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self, weak touch] in
                guard let self, let touch, self.generation == token, self.tracked === touch, self.owner?.active == true else { return }
                self.down(touch)
                guard self.tracked === touch, self.owner?.active == true else { return }
                self.longPressed = true
                if self.pressed && self.owner?.enabled("press") == true {
                    self.pressed = false; self.owner?.send("press", "cancelled", position: self.position(touch))
                }
                guard self.generation == token, self.tracked === touch, self.owner?.active == true else { return }
                self.cancelsTouchesInView = true; self.state = .began
                self.owner?.send("longPress", "started", position: self.position(touch))
            }
        }
    }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let tracked, let view else { return }
        var ancestor: UIView? = view, scrolling = false
        while let item = ancestor { if let scroll = item as? UIScrollView, [.began, .changed].contains(scroll.panGestureRecognizer.state) { scrolling = true }; ancestor = item.superview }
        if scrolling || !view.bounds.insetBy(dx: -20, dy: -20).contains(tracked.location(in: view)) {
            cancelPress(); state = longPressed ? .cancelled : .failed
        }
    }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let tracked, touches.contains(tracked) else { return }
        if !longPressed { down(tracked) }
        let emit = pressed && !longPressed && owner?.enabled("press") == true
        pressed = false; self.tracked = nil; generation &+= 1
        if emit { owner?.send("press", "ended", position: position(tracked), timestamp: tracked.timestamp) }
        state = longPressed ? .ended : .failed
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { cancelPress(); state = .cancelled }
    fileprivate func cancelPress() {
        generation &+= 1
        let touch = tracked, emit = pressed && owner?.enabled("press") == true
        pressed = false; tracked = nil
        if emit, let touch { owner?.send("press", "cancelled", position: position(touch)) }
    }
    override func reset() { cancelPress(); longPressed = false; cancelsTouchesInView = false; super.reset() }
    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
}

@MainActor private final class JadixTouchRecognizer: UIGestureRecognizer {
    private weak var owner: JadixInteractionInput?
    private var touchesByID: [ObjectIdentifier: (UInt32, UITouch)] = [:]
    private static var sequence: UInt32 = 0
    private static let identities = NSMapTable<UITouch, NSNumber>.weakToStrongObjects()
    private static func identity(_ touch: UITouch) -> UInt32 {
        if let value = identities.object(forKey: touch) { return value.uint32Value }
        sequence &+= 1; if sequence == 0 { sequence = 1 }
        identities.setObject(NSNumber(value: sequence), forKey: touch)
        return sequence
    }
    init(owner: JadixInteractionInput) { self.owner = owner; super.init(target: nil, action: nil); cancelsTouchesInView = false; delaysTouchesBegan = false; delaysTouchesEnded = false }
    private func points(_ touches: Set<UITouch>, source: [ObjectIdentifier: (UInt32, UITouch)]) -> [[String: Any]] {
        touches.compactMap { touch -> [String: Any]? in
            guard let (id, _) = source[ObjectIdentifier(touch)] else { return nil }
            let position = touch.location(in: view?.window)
            let force = touch.maximumPossibleForce > 0 ? min(1, max(0, touch.force / touch.maximumPossibleForce)) : 0
            return ["id": id, "position": [position.x, position.y], "force": force,
                    "kind": touch.type == .pencil ? "pencil" : touch.type == .direct ? "direct" : "indirect"]
        }.sorted { ($0["id"] as? UInt32 ?? 0) < ($1["id"] as? UInt32 ?? 0) }
    }
    private func send(_ touches: Set<UITouch>, phase: String, snapshot: [ObjectIdentifier: (UInt32, UITouch)]? = nil,
                      modifiers: UIKeyModifierFlags = []) {
        let changed = points(touches, source: snapshot ?? touchesByID)
        let active = points(Set(touchesByID.values.map { $0.1 }), source: touchesByID)
        guard !changed.isEmpty, let touch = touches.first else { return }
        owner?.send("touch", phase, position: touch.location(in: view?.window), timestamp: touch.timestamp,
                    modifiers: modifiers, fields: ["touches": changed, "activeTouches": active])
    }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        for touch in touches where touchesByID.count < 10 { touchesByID[ObjectIdentifier(touch)] = (Self.identity(touch), touch) }
        send(touches, phase: "started", modifiers: event.modifierFlags)
    }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) { send(touches, phase: "changed", modifiers: event.modifierFlags) }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        // Remove before invoking callbacks; a callback may synchronously unmount.
        let previous = touchesByID
        for touch in touches { touchesByID.removeValue(forKey: ObjectIdentifier(touch)) }
        send(touches, phase: "ended", snapshot: previous, modifiers: event.modifierFlags)
        if touchesByID.isEmpty { state = .failed }
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { cancelTouches(); state = .cancelled }
    fileprivate func cancelTouches() {
        let previous = touchesByID; touchesByID.removeAll()
        send(Set(previous.values.map { $0.1 }), phase: "cancelled", snapshot: previous)
    }
    override func reset() { cancelTouches(); super.reset() }
    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }
}

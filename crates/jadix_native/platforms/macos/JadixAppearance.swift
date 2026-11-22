import AppKit
import QuartzCore
import CoreImage
import CoreText

// Include viewport magnification and every authored ancestor transform. The
// density ceiling bounds bitmap allocations while retaining Retina sharpness.
@MainActor
func jadixBackingDensity(_ view: NSView) -> CGFloat {
    var density = view.window?.backingScaleFactor ?? 1
    var transform = CATransform3DIdentity
    var ancestor: NSView? = view
    while let current = ancestor {
        if let scroll = current as? NSScrollView { density *= scroll.magnification }
        if let layer = current.layer { transform = CATransform3DConcat(transform, layer.transform) }
        ancestor = current.superview
    }
    let x = sqrt(transform.m11 * transform.m11 + transform.m12 * transform.m12 + transform.m13 * transform.m13)
    let y = sqrt(transform.m21 * transform.m21 + transform.m22 * transform.m22 + transform.m23 * transform.m23)
    let scale = density * max(x, y)
    return scale.isFinite ? min(16, max(0.5, scale)) : 1
}

@MainActor
final class JadixViewAppearance {
    weak var view: NSView?
    private let back = JadixDecorationLayer(), front = JadixDecorationLayer(), mask = CAShapeLayer()
    var timer: Timer?
    private var material: NSView?
    private var materialIsGlass = false
    private let gradientMask = JadixGradientMaskLayer()
    private var cursorView: JadixCursorView?
    var motion = JadixVisualMotion()
    var onTransformChange: (() -> Void)?
    var onFrameWork: ((CFTimeInterval) -> Void)?
    var isAnimating: Bool { timer != nil }
    private(set) var current = JadixVisualStyle()
    init(_ view: NSView) {
        self.view = view
        view.wantsLayer = true
        back.zPosition = -1; front.zPosition = 1_000_000; front.front = true
        view.layer?.addSublayer(back); view.layer?.addSublayer(front)
    }
    func cancel() { timer?.invalidate(); timer = nil; motion = JadixVisualMotion() }
    func remove() {
        cancel(); back.removeFromSuperlayer(); front.removeFromSuperlayer()
        view?.layer?.mask = nil; view?.layer?.transform = CATransform3DIdentity; view?.layer?.opacity = 1
        material?.removeFromSuperview(); material = nil; cursorView?.removeFromSuperview(); cursorView = nil
        view?.layer?.filters = nil; view?.layer?.backgroundFilters = nil; view?.layer?.compositingFilter = nil
        view?.layer?.isDoubleSided = true
        view?.isHidden = false
        (view as? NSButton)?.contentTintColor = nil
        (view as? NSImageView)?.contentTintColor = nil
    }
    func render(_ style: JadixVisualStyle) {
        guard let view, let layer = view.layer else { cancel(); return }
        let started = CACurrentMediaTime()
        defer { onFrameWork?(CACurrentMediaTime() - started) }
        current = style
        var style = style
        if NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency && (style.backdrop != nil || style.glass != "none" || !style.backdropFilters.isEmpty) {
            style.background = style.background.alphaComponent == 0 ? .windowBackgroundColor : style.background.withAlphaComponent(1)
            style.backdropFilters = []
        }
        if style.control == "consistent" {
            if style.corners.allSatisfy({ $0 == 0 }) { style.corners = [6, 6, 6, 6] }
            if NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast && style.widths.allSatisfy({ $0 == 0 }) {
                style.widths = [1, 1, 1, 1]; style.colors = Array(repeating: .labelColor, count: 4)
            }
        }
        CATransaction.begin(); CATransaction.setDisableActions(true)
        layer.opacity = Float(style.opacity)
        // Conjugation keeps the view's AppKit frame stable while moving its transform pivot.
        let dx = view.bounds.width * (style.originFraction[0] - 0.5) + style.originOffset[0]
        let dy = view.bounds.height * (style.originFraction[1] - 0.5) + style.originOffset[1]
        let transform = CATransform3DConcat(CATransform3DConcat(CATransform3DMakeTranslation(-dx, -dy, -style.originZ), style.transform), CATransform3DMakeTranslation(dx, dy, style.originZ))
        let transformChanged = !CATransform3DEqualToTransform(layer.transform, transform)
        layer.transform = transform
        layer.isDoubleSided = !style.backfaceHidden; view.isHidden = style.hidden
        layer.compositingFilter = JadixEffectFilters.blend(style.blend)
        layer.filters = JadixEffectFilters.make(style.filters)
        layer.backgroundFilters = JadixEffectFilters.make(style.backdropFilters)
        updateMaterial(style, view: view)
        if style.cursor != "auto" {
            if cursorView == nil { let cursor = JadixCursorView(); cursorView = cursor; view.addSubview(cursor) }
            cursorView?.frame = view.bounds; cursorView?.cursor = JadixCursorView.resolve(style.cursor)
            view.window?.invalidateCursorRects(for: cursorView!)
        } else { cursorView?.removeFromSuperview(); cursorView = nil }
        layer.backgroundColor = nil; layer.cornerRadius = 0
        let extent = max(style.outline + abs(style.outlineOffset), style.shadows.map {
            abs($0.offset.width) + abs($0.offset.height) + $0.blur * 4 + abs($0.spread)
        }.max() ?? 0)
        for decoration in [back, front] {
            decoration.style = style; decoration.frame = view.bounds.insetBy(dx: -extent, dy: -extent)
            decoration.box = CGRect(origin: CGPoint(x: extent, y: extent), size: view.bounds.size)
            decoration.contentsScale = jadixBackingDensity(view)
            decoration.setNeedsDisplay()
        }
        mask.frame = view.bounds; mask.path = style.path(in: view.bounds)
        if !style.masks.isEmpty {
            gradientMask.frame = view.bounds; gradientMask.gradients = style.masks
            gradientMask.contentsScale = jadixBackingDensity(view)
            gradientMask.setNeedsDisplay(); gradientMask.mask = style.clip ? mask : nil; layer.mask = gradientMask
        } else { gradientMask.mask = nil; layer.mask = style.clip ? mask : nil }
        (view as? NSButton)?.contentTintColor = style.accent ?? style.tint
        (view as? NSImageView)?.contentTintColor = style.tint
        CATransaction.commit()
        if transformChanged { onTransformChange?() }
    }
    func refreshBackingDensity() {
        guard let view else { return }
        let density = jadixBackingDensity(view)
        CATransaction.begin(); CATransaction.setDisableActions(true)
        for decoration in [back, front] where decoration.contentsScale != density {
            decoration.contentsScale = density; decoration.setNeedsDisplay()
        }
        if gradientMask.contentsScale != density {
            gradientMask.contentsScale = density; gradientMask.setNeedsDisplay()
        }
        CATransaction.commit()
    }
    private func updateMaterial(_ style: JadixVisualStyle, view: NSView) {
        let reduced = NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency
        guard !reduced, style.backdrop != nil || style.glass != "none" || !style.backdropFilters.isEmpty else {
            material?.removeFromSuperview(); material = nil; return
        }
        let wantsGlass = style.backdrop == nil && style.glass != "none"
        if let old = material, materialIsGlass != wantsGlass { old.removeFromSuperview(); material = nil }
        if material == nil {
            material = wantsGlass ? JadixMakeGlassView(style.glass == "clear", style.glassTint) : nil
            materialIsGlass = wantsGlass
            if material == nil { material = JadixMaterialView(frame: view.bounds) }
            if let material { view.addSubview(material, positioned: .below, relativeTo: nil) }
        }
        guard let material else { return }
        material.frame = view.bounds; material.wantsLayer = true
        let shape = CAShapeLayer(); shape.frame = view.bounds; shape.path = style.path(in: view.bounds); material.layer?.mask = shape
        if let effect = material as? NSVisualEffectView {
            let config = style.backdrop ?? [:]
            effect.material = JadixEffectFilters.material(config["material"] as? String ?? "popover")
            effect.blendingMode = config["blending"] as? String == "behind_window" ? .behindWindow : .withinWindow
            switch config["state"] as? String { case "inactive": effect.state = .inactive
            case "follows_window_active_state": effect.state = .followsWindowActiveState
            default: effect.state = .active }
        } else { JadixUpdateGlassView(material, style.glass == "clear", style.glassTint) }
    }
    deinit { timer?.invalidate() }
}


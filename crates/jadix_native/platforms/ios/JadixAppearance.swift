import UIKit
import QuartzCore

@MainActor final class JadixViewAppearance {
    weak var view: UIView?
    var timer: Timer?
    var motion = JadixVisualMotion()
    private let back = JadixDecorationLayer(), front = JadixDecorationLayer(), mask = CAShapeLayer()
    private let gradientMask = JadixGradientMaskLayer()
    private var material: UIVisualEffectView?
    private var raster: JadixRasterEffects?
    private(set) var current = JadixVisualStyle()
    var onTransformChange: (() -> Void)?
    var isAnimating: Bool { timer != nil }
    init(_ view: UIView) {
        self.view = view
        back.zPosition = -1; front.zPosition = 1_000_000; front.front = true
        view.layer.addSublayer(back); view.layer.addSublayer(front)
    }
    func cancel() { timer?.invalidate(); timer = nil; motion = JadixVisualMotion() }
    func remove() {
        raster?.remove(); raster = nil
        cancel(); back.removeFromSuperlayer(); front.removeFromSuperlayer(); material?.removeFromSuperview(); material = nil
        view?.layer.mask = nil; view?.layer.transform = CATransform3DIdentity; view?.layer.opacity = 1
    }
    func render(_ requested: JadixVisualStyle) {
        guard let view else { cancel(); return }
        current = requested
        var style = requested
        if UIAccessibility.isReduceTransparencyEnabled && (style.backdrop != nil || style.glass != "none") {
            style.background = style.background.withAlphaComponent(1)
        }
        if style.control == "consistent", style.corners.allSatisfy({ $0 == 0 }) { style.corners = [6,6,6,6] }
        let layer = view.layer
        CATransaction.begin(); CATransaction.setDisableActions(true)
        layer.opacity = Float(style.opacity); view.isHidden = style.hidden; layer.isDoubleSided = !style.backfaceHidden
        let dx = view.bounds.width * (style.originFraction[0] - 0.5) + style.originOffset[0]
        let dy = view.bounds.height * (style.originFraction[1] - 0.5) + style.originOffset[1]
        let transform = CATransform3DConcat(CATransform3DConcat(CATransform3DMakeTranslation(-dx,-dy,-style.originZ),style.transform),CATransform3DMakeTranslation(dx,dy,style.originZ))
        let changed = !CATransform3DEqualToTransform(layer.transform, transform)
        layer.transform = transform; layer.backgroundColor = nil
        let extent = max(style.outline + abs(style.outlineOffset), style.shadows.map {
            abs($0.offset.width) + abs($0.offset.height) + $0.blur * 4 + abs($0.spread)
        }.max() ?? 0)
        for decoration in [back,front] {
            decoration.style = style; decoration.frame = view.bounds.insetBy(dx: -extent, dy: -extent)
            decoration.box = CGRect(origin: CGPoint(x: extent,y: extent),size: view.bounds.size)
            decoration.contentsScale = view.window?.screen.scale ?? view.traitCollection.displayScale
            decoration.setNeedsDisplay()
        }
        mask.frame = view.bounds; mask.path = style.path(in: view.bounds)
        if !style.masks.isEmpty {
            gradientMask.frame = view.bounds; gradientMask.gradients = style.masks
            gradientMask.contentsScale = back.contentsScale; gradientMask.setNeedsDisplay()
            gradientMask.mask = style.clip ? mask : nil; layer.mask = gradientMask
        } else { gradientMask.mask = nil; layer.mask = style.clip ? mask : nil }
        if !UIAccessibility.isReduceTransparencyEnabled && (style.backdrop != nil || style.glass != "none" || !style.backdropFilters.isEmpty) {
            if material == nil {
                let effect = UIVisualEffectView(effect: UIBlurEffect(style: .systemMaterial))
                effect.isUserInteractionEnabled = false; view.insertSubview(effect, at: 0); material = effect
            }
            material?.frame = view.bounds; material?.contentView.backgroundColor = style.glassTint
            // Visual effects must be clipped through maskView, so the sampled
            // backdrop obeys the same shape as the visible content (#1575).
            let effectMask = UIView(frame: view.bounds); effectMask.backgroundColor = .black
            let shape = CAShapeLayer(); shape.frame = effectMask.bounds; shape.path = style.path(in: effectMask.bounds)
            effectMask.layer.mask = shape; material?.mask = effectMask
        } else { material?.removeFromSuperview(); material = nil }
        view.tintColor = style.accent ?? style.tint
        CATransaction.commit()
        if raster == nil { raster = JadixRasterEffects(view) }
        raster?.update(style)
        if changed { onTransformChange?() }
    }
    deinit { timer?.invalidate() }
}

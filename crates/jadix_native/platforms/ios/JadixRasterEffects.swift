import UIKit
import CoreImage

/// Public Core Image composition. The source view retains input/accessibility
/// ownership; a sibling layer displays its filtered pixels without filtering
/// UIKit's private layers. All temporary visibility changes are synchronous.
@MainActor final class JadixRasterEffects {
    private static let context = CIContext(options: [.cacheIntermediates: false])
    weak var view: UIView?
    private let output = CALayer()
    private var timer: Timer?
    private var style = JadixVisualStyle()
    init(_ view: UIView) { self.view = view }
    func update(_ style: JadixVisualStyle) {
        self.style = style
        let enabled = !style.filters.isEmpty || !style.backdropFilters.isEmpty || style.blend != "normal"
        guard enabled else { remove(); return }
        if timer == nil {
            let timer = Timer(timeInterval: 1.0 / 30, repeats: true) { [weak self] _ in self?.render() }
            self.timer = timer; RunLoop.main.add(timer, forMode: .common)
        }
        render()
    }
    private func capture(_ layer: CALayer, rect: CGRect, scale: CGFloat, translate: CGPoint = .zero) -> CGImage? {
        let format = UIGraphicsImageRendererFormat(); format.scale = scale; format.opaque = false
        return UIGraphicsImageRenderer(size: rect.size, format: format).image { c in
            c.cgContext.translateBy(x: -rect.minX + translate.x, y: -rect.minY + translate.y)
            layer.render(in: c.cgContext)
        }.cgImage
    }
    private func render() {
        guard let view, let parent = view.layer.superlayer, view.window != nil,
              !view.isHidden, view.bounds.width > 0, view.bounds.height > 0 else { output.isHidden = true; return }
        let padding = min(256, style.filters.reduce(CGFloat(0)) { result, item in
            switch item["kind"] as? String {
            case "blur": return result + CGFloat(item["value"] as? Double ?? 0) * 3
            case "drop_shadow":
                let value = item["value"] as? [String: Any] ?? [:]
                let offset = value["offset"] as? [Double] ?? [0, 0]
                return result + CGFloat(value["blur"] as? Double ?? 0) * 3 + CGFloat(abs(value["spread"] as? Double ?? 0)) + CGFloat(offset.map(abs).max() ?? 0)
            default: return result
            }
        })
        let rect = view.bounds.insetBy(dx: -padding, dy: -padding)
        let scale = min(view.window?.screen.scale ?? 1, 2048 / max(rect.width, rect.height))
        guard scale > 0, rect.width * rect.height * scale * scale <= 4_194_304 else { return }
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        output.isHidden = true
        let source = view.layer, transform = source.transform, opacity = source.opacity
        source.transform = CATransform3DIdentity; source.opacity = 1
        let foreground = capture(source, rect: rect, scale: scale)
        source.transform = transform; source.opacity = opacity
        guard let foreground else { source.opacity = Float(style.opacity); return }
        var image = JadixEffectFilters.filtered(CIImage(cgImage: foreground), values: style.filters, scale: scale)
        if !style.backdropFilters.isEmpty || style.blend != "normal" {
            let siblings = parent.sublayers ?? []
            let index = siblings.firstIndex(where: { $0 === source }) ?? siblings.count
            let hidden = siblings.enumerated().filter { $0.offset >= index }.map { ($0.element, $0.element.isHidden) }
            for (layer, _) in hidden { layer.isHidden = true }
            source.transform = CATransform3DIdentity
            let origin = source.convert(CGPoint.zero, to: parent)
            let backdrop = capture(parent, rect: rect, scale: scale, translate: CGPoint(x: -origin.x, y: -origin.y))
            source.transform = transform
            for (layer, wasHidden) in hidden { layer.isHidden = wasHidden }
            if let backdrop {
                let background = JadixEffectFilters.filtered(CIImage(cgImage: backdrop), values: UIAccessibility.isReduceTransparencyEnabled ? [] : style.backdropFilters, scale: scale)
                image = JadixEffectFilters.composite(image, over: background, mode: style.blend)
            }
        }
        let extent = CGRect(origin: .zero, size: CGSize(width: rect.width * scale, height: rect.height * scale))
        guard let pixels = Self.context.createCGImage(image, from: extent) else { source.opacity = Float(style.opacity); return }
        output.removeFromSuperlayer(); parent.insertSublayer(output, above: source)
        output.bounds = CGRect(origin: .zero, size: rect.size); output.position = source.position
        output.anchorPoint = CGPoint(x: (source.bounds.minX + source.bounds.width * source.anchorPoint.x - rect.minX) / rect.width,
                                     y: (source.bounds.minY + source.bounds.height * source.anchorPoint.y - rect.minY) / rect.height)
        output.zPosition = source.zPosition
        output.transform = transform; output.contentsScale = scale; output.contents = pixels
        output.opacity = Float(style.opacity); output.isDoubleSided = source.isDoubleSided; output.isHidden = false
        source.opacity = 0
    }
    func remove() { timer?.invalidate(); timer = nil; output.removeFromSuperlayer(); output.contents = nil; view?.layer.opacity = Float(style.opacity) }
    deinit { timer?.invalidate() }
    static func blur(_ image: CGImage, radius: CGFloat) -> CGImage {
        guard radius > 0 else { return image }
        let source = CIImage(cgImage: image)
        let result = source.clampedToExtent().applyingFilter("CIGaussianBlur", parameters: [kCIInputRadiusKey: min(256, radius)])
        return context.createCGImage(result, from: source.extent) ?? image
    }
}

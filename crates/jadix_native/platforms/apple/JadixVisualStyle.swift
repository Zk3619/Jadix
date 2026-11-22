import QuartzCore
import CoreImage
#if os(iOS)
import UIKit
typealias JadixColor = UIColor
enum JadixColorSpace { case sRGB }
extension UIColor {
    convenience init(srgbRed: CGFloat, green: CGFloat, blue: CGFloat, alpha: CGFloat) { self.init(red: srgbRed, green: green, blue: blue, alpha: alpha) }
    func usingColorSpace(_ space: JadixColorSpace) -> UIColor? {
        guard let value = cgColor.converted(to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent, options: nil) else { return nil }
        return UIColor(cgColor: value)
    }
    private var rgba: [CGFloat] { var r: CGFloat=0, g: CGFloat=0, b: CGFloat=0, a: CGFloat=0; getRed(&r, green: &g, blue: &b, alpha: &a); return [r,g,b,a] }
    var redComponent: CGFloat { rgba[0] }; var greenComponent: CGFloat { rgba[1] }
    var blueComponent: CGFloat { rgba[2] }; var alphaComponent: CGFloat { rgba[3] }
}
#else
import AppKit
typealias JadixColor = NSColor
#endif

// The wire representation is shared with jadix_core::presentation::style.
struct JadixVisualStyle {
    var opacity: CGFloat = 1
    var matrix: [CGFloat] = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
    var background: JadixColor = .clear
    var tint: JadixColor?
    var corners: [CGFloat] = [0, 0, 0, 0]
    var widths: [CGFloat] = [0, 0, 0, 0]
    var colors: [JadixColor] = Array(repeating: .clear, count: 4)
    var outline: CGFloat = 0
    var outlineOffset: CGFloat = 0
    var outlineColor: JadixColor = .clear
    var clip = false
    var continuous = false, backfaceHidden = false, hidden = false
    var originFraction: [CGFloat] = [0.5, 0.5], originOffset: [CGFloat] = [0, 0], originZ: CGFloat = 0
    var borderStyle = "solid", outlineStyle = "solid", blend = "normal", cursor = "auto", control = "native"
    var accent: JadixColor?, glassTint: JadixColor?
    var backgrounds: [JadixGradient] = [], masks: [JadixGradient] = []
    var filters: [[String: Any]] = [], backdropFilters: [[String: Any]] = []
    var backdrop: [String: Any]?, glass = "none"
    struct Shadow {
        var offset: CGSize = .zero
        var blur: CGFloat = 0
        var spread: CGFloat = 0
        var color: JadixColor = .clear
        var inset = false
    }
    var shadows: [Shadow] = []

    static func color(_ value: Any?) -> JadixColor? {
        guard let rgba = value as? [Double], rgba.count == 4 else { return nil }
        return JadixColor(srgbRed: rgba[0] / 255, green: rgba[1] / 255,
                       blue: rgba[2] / 255, alpha: rgba[3] / 255)
    }

    init(_ value: [String: Any]? = nil, background fallback: JadixColor = .clear) {
        background = fallback
        guard let value else { return }
        opacity = CGFloat(value["opacity"] as? Double ?? 1)
        if let values = value["transform"] as? [Double], values.count == 16 { matrix = values.map { CGFloat($0) } }
        background = Self.color(value["background"]) ?? fallback
        tint = Self.color(value["tint"])
        if let values = value["corners"] as? [Double], values.count == 4 { corners = values.map { CGFloat($0) } }
        if let values = value["border_widths"] as? [Double], values.count == 4 { widths = values.map { CGFloat($0) } }
        if let values = value["border_colors"] as? [[Double]], values.count == 4 {
            colors = values.map { Self.color($0) ?? .clear }
        }
        outline = CGFloat(value["outline_width"] as? Double ?? 0)
        outlineOffset = CGFloat(value["outline_offset"] as? Double ?? 0)
        outlineColor = Self.color(value["outline_color"]) ?? .clear
        clip = value["clip"] as? Bool ?? false
        continuous = value["continuous_corners"] as? Bool ?? false
        backfaceHidden = value["backface_hidden"] as? Bool ?? false; hidden = value["hidden"] as? Bool ?? false
        let origin = value["origin"] as? [String: Any] ?? [:]
        if let v = origin["fraction"] as? [Double], v.count == 2 { originFraction = v.map { CGFloat($0) } }
        if let v = origin["offset"] as? [Double], v.count == 2 { originOffset = v.map { CGFloat($0) } }
        originZ = CGFloat(origin["z"] as? Double ?? 0)
        borderStyle = value["border_style"] as? String ?? "solid"; outlineStyle = value["outline_style"] as? String ?? "solid"
        blend = value["blend_mode"] as? String ?? "normal"; cursor = value["cursor"] as? String ?? "auto"
        control = value["control"] as? String ?? "native"; accent = Self.color(value["accent"])
        backgrounds = (value["backgrounds"] as? [[String: Any]] ?? []).compactMap(JadixGradient.init)
        masks = (value["masks"] as? [[String: Any]] ?? []).compactMap(JadixGradient.init)
        filters = value["filters"] as? [[String: Any]] ?? []; backdropFilters = value["backdrop_filters"] as? [[String: Any]] ?? []
        backdrop = value["backdrop"] as? [String: Any]; glass = value["glass"] as? String ?? "none"
        glassTint = Self.color(value["glass_tint"])
        shadows = (value["shadows"] as? [[String: Any]] ?? []).map { entry in
            var shadow = Shadow()
            if let offset = entry["offset"] as? [Double], offset.count == 2 {
                shadow.offset = CGSize(width: offset[0], height: offset[1])
            }
            shadow.blur = CGFloat(entry["blur"] as? Double ?? 0)
            shadow.spread = CGFloat(entry["spread"] as? Double ?? 0)
            shadow.color = Self.color(entry["color"]) ?? .clear
            shadow.inset = entry["inset"] as? Bool ?? false
            return shadow
        }
    }

    var transform: CATransform3D {
        CATransform3D(m11: matrix[0], m12: matrix[1], m13: matrix[2], m14: matrix[3],
                      m21: matrix[4], m22: matrix[5], m23: matrix[6], m24: matrix[7],
                      m31: matrix[8], m32: matrix[9], m33: matrix[10], m34: matrix[11],
                      m41: matrix[12], m42: matrix[13], m43: matrix[14], m44: matrix[15])
    }

    // Premultiplied interpolation avoids dark fringes when a color fades to transparent.
    func interpolated(to end: Self, at t: CGFloat) -> Self {
        func mix(_ a: CGFloat, _ b: CGFloat) -> CGFloat { a + (b - a) * t }
        func color(_ a: JadixColor, _ b: JadixColor) -> JadixColor {
            let a = a.usingColorSpace(.sRGB) ?? .clear, b = b.usingColorSpace(.sRGB) ?? .clear
            let alpha = mix(a.alphaComponent, b.alphaComponent)
            guard alpha > 0 else { return .clear }
            func channel(_ a: CGFloat, _ b: CGFloat) -> CGFloat { min(1, max(0, mix(a, b) / alpha)) }
            return JadixColor(srgbRed: channel(a.redComponent * a.alphaComponent, b.redComponent * b.alphaComponent),
                           green: channel(a.greenComponent * a.alphaComponent, b.greenComponent * b.alphaComponent),
                           blue: channel(a.blueComponent * a.alphaComponent, b.blueComponent * b.alphaComponent), alpha: min(1, alpha))
        }
        var result = t < 0.5 ? self : end
        result.originFraction = zip(originFraction, end.originFraction).map { mix($0.0, $0.1) }
        result.originOffset = zip(originOffset, end.originOffset).map { mix($0.0, $0.1) }
        result.originZ = mix(originZ, end.originZ)
        result.backgrounds = JadixGradient.interpolate(backgrounds, end.backgrounds, t)
        result.masks = JadixGradient.interpolate(masks, end.masks, t)
        result.filters = JadixEffectFilters.interpolate(filters, end.filters, t)
        result.backdropFilters = JadixEffectFilters.interpolate(backdropFilters, end.backdropFilters, t)
        if let accent, let b = end.accent { result.accent = color(accent, b) }
        if let glassTint, let b = end.glassTint { result.glassTint = color(glassTint, b) }
        result.opacity = min(1, max(0, mix(opacity, end.opacity)))
        result.matrix = zip(matrix, end.matrix).map { mix($0.0, $0.1) }
        result.background = color(background, end.background)
        if let tint, let endTint = end.tint { result.tint = color(tint, endTint) }
        result.corners = zip(corners, end.corners).map { max(0, mix($0.0, $0.1)) }
        result.widths = zip(widths, end.widths).map { max(0, mix($0.0, $0.1)) }
        result.colors = zip(colors, end.colors).map { color($0.0, $0.1) }
        result.outline = max(0, mix(outline, end.outline))
        result.outlineOffset = mix(outlineOffset, end.outlineOffset)
        result.outlineColor = color(outlineColor, end.outlineColor)
        result.shadows = (0..<max(shadows.count, end.shadows.count)).map { i in
            var a = i < shadows.count ? shadows[i] : Shadow()
            var b = i < end.shadows.count ? end.shadows[i] : Shadow()
            if i >= shadows.count { a.inset = b.inset }
            if i >= end.shadows.count { b.inset = a.inset }
            guard a.inset == b.inset else { return t < 0.5 ? a : b }
            return Shadow(offset: CGSize(width: mix(a.offset.width, b.offset.width), height: mix(a.offset.height, b.offset.height)),
                          blur: max(0, mix(a.blur, b.blur)), spread: mix(a.spread, b.spread), color: color(a.color, b.color), inset: a.inset)
        }
        return result
    }

    func path(in rect: CGRect, expansion: CGFloat = 0) -> CGPath {
        let box = rect.insetBy(dx: -expansion, dy: -expansion)
        let radii = corners.map { max(0, $0 + expansion) }
        let scale = min(1, min(box.width / max(1, max(radii[0] + radii[1], radii[2] + radii[3])),
                               box.height / max(1, max(radii[0] + radii[3], radii[1] + radii[2]))))
        let r = radii.map { $0 * max(0, scale) }, p = CGMutablePath()
        if continuous {
            let k: CGFloat = 0.86
            p.move(to: CGPoint(x: box.minX + r[0], y: box.minY))
            p.addLine(to: CGPoint(x: box.maxX - r[1], y: box.minY))
            p.addCurve(to: CGPoint(x: box.maxX, y: box.minY + r[1]), control1: CGPoint(x: box.maxX - r[1] * (1-k), y: box.minY), control2: CGPoint(x: box.maxX, y: box.minY + r[1] * (1-k)))
            p.addLine(to: CGPoint(x: box.maxX, y: box.maxY - r[2]))
            p.addCurve(to: CGPoint(x: box.maxX - r[2], y: box.maxY), control1: CGPoint(x: box.maxX, y: box.maxY - r[2] * (1-k)), control2: CGPoint(x: box.maxX - r[2] * (1-k), y: box.maxY))
            p.addLine(to: CGPoint(x: box.minX + r[3], y: box.maxY))
            p.addCurve(to: CGPoint(x: box.minX, y: box.maxY - r[3]), control1: CGPoint(x: box.minX + r[3] * (1-k), y: box.maxY), control2: CGPoint(x: box.minX, y: box.maxY - r[3] * (1-k)))
            p.addLine(to: CGPoint(x: box.minX, y: box.minY + r[0]))
            p.addCurve(to: CGPoint(x: box.minX + r[0], y: box.minY), control1: CGPoint(x: box.minX, y: box.minY + r[0] * (1-k)), control2: CGPoint(x: box.minX + r[0] * (1-k), y: box.minY))
            p.closeSubpath(); return p
        }
        p.move(to: CGPoint(x: box.minX + r[0], y: box.minY))
        p.addArc(tangent1End: CGPoint(x: box.maxX, y: box.minY), tangent2End: CGPoint(x: box.maxX, y: box.maxY), radius: r[1])
        p.addArc(tangent1End: CGPoint(x: box.maxX, y: box.maxY), tangent2End: CGPoint(x: box.minX, y: box.maxY), radius: r[2])
        p.addArc(tangent1End: CGPoint(x: box.minX, y: box.maxY), tangent2End: CGPoint(x: box.minX, y: box.minY), radius: r[3])
        p.addArc(tangent1End: CGPoint(x: box.minX, y: box.minY), tangent2End: CGPoint(x: box.maxX, y: box.minY), radius: r[0])
        p.closeSubpath()
        return p
    }
}

final class JadixDecorationLayer: CALayer {
    var style = JadixVisualStyle()
    var front = false
    var box: CGRect = .zero
    override func draw(in context: CGContext) {
        let shape = style.path(in: box)
        for shadow in style.shadows.reversed() where shadow.inset == front {
            context.saveGState()
            let extent = max(bounds.width, bounds.height) + abs(shadow.spread) + shadow.blur * 4 + 1
            if shadow.inset {
                context.addPath(shape); context.clip()
                let hole = style.path(in: box.offsetBy(dx: shadow.offset.width, dy: shadow.offset.height), expansion: -shadow.spread)
                context.addRect(box.insetBy(dx: -extent, dy: -extent)); context.addPath(hole)
                context.setShadow(offset: .zero, blur: shadow.blur, color: shadow.color.cgColor)
                context.setFillColor(shadow.color.cgColor); context.drawPath(using: .eoFill)
            } else {
                context.addRect(bounds); context.addPath(shape); context.clip(using: .evenOdd)
                context.setShadow(offset: shadow.offset, blur: shadow.blur, color: shadow.color.cgColor)
                context.addPath(style.path(in: box, expansion: shadow.spread))
                context.setFillColor(JadixColor.black.cgColor); context.fillPath()
            }
            context.restoreGState()
        }
        if !front {
            context.addPath(shape); context.setFillColor(style.background.cgColor); context.fillPath()
            context.saveGState(); context.addPath(shape); context.clip()
            for gradient in style.backgrounds.reversed() { gradient.draw(in: context, box: box) }
            context.restoreGState()
            return
        }
        let left = min(box.width, style.widths[3]), top = min(box.height, style.widths[0])
        let inner = CGRect(x: box.minX + left, y: box.minY + top,
                           width: max(0, box.width - left - style.widths[1]),
                           height: max(0, box.height - top - style.widths[2]))
        var innerStyle = style
        innerStyle.corners = [max(0, style.corners[0] - max(style.widths[0], style.widths[3])),
                              max(0, style.corners[1] - max(style.widths[0], style.widths[1])),
                              max(0, style.corners[2] - max(style.widths[2], style.widths[1])),
                              max(0, style.corners[3] - max(style.widths[2], style.widths[3]))]
        let outer = [CGPoint(x: box.minX, y: box.minY), CGPoint(x: box.maxX, y: box.minY),
                     CGPoint(x: box.maxX, y: box.maxY), CGPoint(x: box.minX, y: box.maxY)]
        let inside = [CGPoint(x: inner.minX, y: inner.minY), CGPoint(x: inner.maxX, y: inner.minY),
                      CGPoint(x: inner.maxX, y: inner.maxY), CGPoint(x: inner.minX, y: inner.maxY)]
        for i in 0..<4 where style.widths[i] > 0 {
            context.saveGState()
            context.addPath(shape); context.addPath(innerStyle.path(in: inner)); context.clip(using: .evenOdd)
            if style.borderStyle != "solid" {
                context.move(to: outer[i]); context.addLine(to: outer[(i + 1) % 4]); context.addLine(to: inside[(i + 1) % 4]); context.addLine(to: inside[i]); context.closePath(); context.clip()
                context.setLineWidth(style.widths[i] * 2)
                context.setLineCap(style.borderStyle == "dotted" ? .round : .butt)
                context.setLineDash(phase: 0, lengths: style.borderStyle == "dotted" ? [0, style.widths[i] * 2] : [style.widths[i] * 3, style.widths[i] * 2])
                context.setStrokeColor(style.colors[i].cgColor)
                context.addPath(shape); context.strokePath(); context.restoreGState(); continue
            }
            context.move(to: outer[i]); context.addLine(to: outer[(i + 1) % 4])
            context.addLine(to: inside[(i + 1) % 4]); context.addLine(to: inside[i]); context.closePath()
            context.setFillColor(style.colors[i].cgColor); context.fillPath(); context.restoreGState()
        }
        if style.outline > 0 {
            context.addPath(style.path(in: box, expansion: style.outlineOffset + style.outline / 2))
            context.setLineDash(phase: 0, lengths: style.outlineStyle == "solid" ? [] : style.outlineStyle == "dotted" ? [0, style.outline * 2] : [style.outline * 3, style.outline * 2])
            context.setLineCap(style.outlineStyle == "dotted" ? .round : .butt)
            context.setLineWidth(style.outline); context.setStrokeColor(style.outlineColor.cgColor); context.strokePath()
        }
    }
}

// One clock composes style transitions with keyframes. Targets and the unmodified
// visible base are separate, so snapshots and interrupted transitions never compound a pulse/scale.
struct JadixVisualMotion {
    var wire: NSDictionary?, background: JadixColor?
    var target: JadixVisualStyle?, base: JadixVisualStyle?
    var from: JadixVisualStyle?, transition: [String: Any]?
    var animationWire: NSDictionary?, animation: [String: Any]?
    var elapsed: Double = 0, animationElapsed: Double = 0, last: Double = 0
    var interval: Double = 0

    static func ease(_ points: [Double], _ x: Double) -> CGFloat {
        let p = points.count == 4 ? points : [0, 0, 1, 1]
        func cubic(_ t: Double, _ a: Double, _ b: Double) -> Double {
            3 * (1-t) * (1-t) * t * a + 3 * (1-t) * t * t * b + t * t * t
        }
        // Monotone x permits bisection even for zero endpoint derivatives.
        var low = 0.0, high = 1.0
        for _ in 0..<24 {
            let mid = (low + high) / 2
            if cubic(mid, p[0], p[2]) < x { low = mid } else { high = mid }
        }
        return CGFloat(cubic((low + high) / 2, p[1], p[3]))
    }
    static func spring(_ config: [String: Any], _ seconds: Double) -> (CGFloat, Bool) {
        let k = config["stiffness"] as? Double ?? 100, c = config["damping"] as? Double ?? 10
        let mass = config["mass"] as? Double ?? 1, w = sqrt(k / mass), decay = c / (2 * mass)
        let x: Double, velocity: Double
        if abs(decay - w) <= w * 0.000001 {
            let e = exp(-w * seconds)
            x = (1 + w * seconds) * e; velocity = -w * w * seconds * e
        } else if decay < w {
            let d = sqrt(w * w - decay * decay), e = exp(-decay * seconds)
            x = e * (cos(d * seconds) + decay / d * sin(d * seconds))
            velocity = -e * w * w / d * sin(d * seconds)
        } else {
            let d = sqrt(decay * decay - w * w)
            let slow = -w * w / (decay + d), fast = -decay - d
            let a = -fast / (slow-fast), b = slow / (slow-fast)
            x = a * exp(slow * seconds) + b * exp(fast * seconds)
            velocity = a * slow * exp(slow * seconds) + b * fast * exp(fast * seconds)
        }
        return (CGFloat(1-x), abs(x) < 0.001 && abs(velocity) < 0.001)
    }
    static func transition(_ from: JadixVisualStyle, _ end: JadixVisualStyle, _ t: CGFloat, _ props: Int) -> JadixVisualStyle {
        let tween = from.interpolated(to: end, at: t)
        var result = end
        if props & 1 != 0 {
            result.background = tween.background; result.tint = tween.tint; result.accent = tween.accent
            result.colors = tween.colors; result.outlineColor = tween.outlineColor
            result.glassTint = tween.glassTint; result.backgrounds = tween.backgrounds
        }
        if props & 2 != 0 { result.opacity = tween.opacity }
        if props & 4 != 0 {
            result.matrix = tween.matrix; result.originFraction = tween.originFraction
            result.originOffset = tween.originOffset; result.originZ = tween.originZ
        }
        if props & 8 != 0 { result.shadows = tween.shadows }
        if props & 16 != 0 {
            result.corners = tween.corners; result.widths = tween.widths
            result.outline = tween.outline; result.outlineOffset = tween.outlineOffset; result.masks = tween.masks
        }
        if props == 31 { result.filters = tween.filters; result.backdropFilters = tween.backdropFilters }
        return result
    }
    static func keyframes(_ config: [String: Any], _ elapsed: Double, _ style: JadixVisualStyle, _ height: CGFloat) -> (JadixVisualStyle, Bool) {
        let after = elapsed - (config["delay"] as? Double ?? 0) / 1000
        let duration = (config["duration"] as? Double ?? 1000) / 1000
        if let iterations = config["iterations"] as? Double, after >= duration * iterations { return (style, false) }
        guard after >= 0, duration > 0 else { return (style, duration > 0) }
        let frac = after.truncatingRemainder(dividingBy: duration) / duration
        let points = config["easing"] as? [Double] ?? [0, 0, 1, 1]
        func ease(_ x: Double) -> CGFloat { Self.ease(points, x) }
        var result = style, extra = CATransform3DIdentity
        switch config["keyframes"] as? String ?? "spin" {
        case "ping":
            let q = frac < 0.75 ? ease(frac / 0.75) : 1
            extra = CATransform3DMakeScale(1+q, 1+q, 1); result.opacity *= max(0, 1-q)
        case "pulse":
            let q = frac < 0.5 ? ease(frac * 2) : 1-ease(frac * 2-1)
            result.opacity *= min(1, max(0, 1-0.5*q))
        case "bounce":
            let q = frac < 0.5 ? 1-Self.ease([0.8, 0, 1, 1], frac * 2) : Self.ease([0, 0, 0.2, 1], frac * 2-1)
            extra = CATransform3DMakeTranslation(0, -0.25 * height * q, 0)
        default: extra = CATransform3DMakeRotation(ease(frac) * 2 * .pi, 0, 0, 1)
        }
        let m = CATransform3DConcat(extra, style.transform)
        result.matrix = [m.m11,m.m12,m.m13,m.m14,m.m21,m.m22,m.m23,m.m24,m.m31,m.m32,m.m33,m.m34,m.m41,m.m42,m.m43,m.m44]
        return (result, true)
    }
}


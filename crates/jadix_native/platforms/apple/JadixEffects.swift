#if os(iOS)
import UIKit
#else
import AppKit
#endif
import QuartzCore
import CoreImage
import ObjectiveC

private func mixColor(_ a: JadixColor, _ b: JadixColor, _ t: CGFloat) -> JadixColor {
    let a = a.usingColorSpace(.sRGB) ?? .clear, b = b.usingColorSpace(.sRGB) ?? .clear
    let alpha = a.alphaComponent + (b.alphaComponent - a.alphaComponent) * t
    guard alpha > 0 else { return .clear }
    func channel(_ a: CGFloat, _ b: CGFloat) -> CGFloat { min(1, max(0, (a + (b - a) * t) / alpha)) }
    return JadixColor(srgbRed: channel(a.redComponent * a.alphaComponent, b.redComponent * b.alphaComponent),
                   green: channel(a.greenComponent * a.alphaComponent, b.greenComponent * b.alphaComponent),
                   blue: channel(a.blueComponent * a.alphaComponent, b.blueComponent * b.alphaComponent), alpha: min(1, alpha))
}

struct JadixGradient {
    var geometry: [String: Any]
    var colors: [JadixColor]
    var positions: [[String: Double]?]
    var repeating: Bool
    init?(_ value: [String: Any]) {
        guard let geometry = value["geometry"] as? [String: Any], let stops = value["stops"] as? [[String: Any]], stops.count >= 2 else { return nil }
        self.geometry = geometry
        colors = stops.map { JadixVisualStyle.color($0["color"]) ?? .clear }
        positions = stops.map { $0["position"] as? [String: Double] }
        repeating = value["repeating"] as? Bool ?? false
    }
    static func interpolate(_ a: [Self], _ b: [Self], _ t: CGFloat) -> [Self] {
        guard a.count == b.count else { return t < 0.5 ? a : b }
        return zip(a, b).map { a, b in
            guard a.geometry["kind"] as? String == b.geometry["kind"] as? String,
                  a.colors.count == b.colors.count, a.repeating == b.repeating else { return t < 0.5 ? a : b }
            var result = t < 0.5 ? a : b
            for key in ["angle"] {
                if let x = a.geometry[key] as? Double, let y = b.geometry[key] as? Double { result.geometry[key] = x + (y - x) * Double(t) }
            }
            if let x = a.geometry["center"] as? [Double], let y = b.geometry["center"] as? [Double], x.count == y.count {
                result.geometry["center"] = zip(x, y).map { $0 + ($1 - $0) * Double(t) }
            }
            result.colors = zip(a.colors, b.colors).map { mixColor($0, $1, t) }
            result.positions = zip(a.positions, b.positions).map { x, y in
                guard let x, let y, let key = x.keys.first, let from = x[key], let to = y[key] else { return t < 0.5 ? x : y }
                return [key: from + (to - from) * Double(t)]
            }
            return result
        }
    }
    private func stops(length: CGFloat) -> [(CGFloat, JadixColor)] {
        var values: [CGFloat?] = positions.map { p in
            if let fraction = p?["fraction"] { return CGFloat(fraction) }
            if let points = p?["points"] { return length > 0 ? CGFloat(points) / length : 0 }
            return nil
        }
        if values[0] == nil { values[0] = 0 }; if values[values.count - 1] == nil { values[values.count - 1] = 1 }
        var previous = -CGFloat.infinity
        for i in values.indices { if let value = values[i] { values[i] = max(previous, value); previous = values[i]! } }
        var start = 0
        for end in 1..<values.count where values[end] != nil {
            for i in (start + 1)..<end { values[i] = values[start]! + (values[end]! - values[start]!) * CGFloat(i - start) / CGFloat(end - start) }
            start = end
        }
        return zip(values, colors).map { ($0!, $1) }
    }
    private func sample(_ x: CGFloat, _ stops: [(CGFloat, JadixColor)]) -> JadixColor {
        var value = x
        if repeating {
            let low = stops.first!.0, period = stops.last!.0 - low
            if period <= 0.0001 { return stops.last!.1 }
            value = low + (value - low).truncatingRemainder(dividingBy: period)
            if value < low { value += period }
        }
        if value <= stops.first!.0 { return stops.first!.1 }
        for i in 1..<stops.count where value <= stops[i].0 {
            let distance = stops[i].0 - stops[i - 1].0
            return mixColor(stops[i - 1].1, stops[i].1, distance > 0 ? (value - stops[i - 1].0) / distance : 1)
        }
        return stops.last!.1
    }
    func draw(in context: CGContext, box: CGRect) {
        guard box.width > 0, box.height > 0 else { return }
        let kind = geometry["kind"] as? String ?? "linear"
        let center = geometry["center"] as? [Double] ?? [0.5, 0.5]
        let origin = CGPoint(x: box.minX + box.width * CGFloat(center[0]), y: box.minY + box.height * CGFloat(center[1]))
        var angle = CGFloat(geometry["angle"] as? Double ?? 0)
        if kind == "linear_corner" {
            angle = atan2(box.height * (geometry["right"] as? Bool == true ? 1 : -1), box.width * (geometry["bottom"] as? Bool == true ? -1 : 1))
        }
        let dx = sin(angle), dy = -cos(angle)
        var length = abs(box.width * dx) + abs(box.height * dy), rx = box.width / 2, ry = box.height / 2
        if kind == "radial" {
            let xs = [abs(origin.x - box.minX), abs(box.maxX - origin.x)]
            let ys = [abs(origin.y - box.minY), abs(box.maxY - origin.y)]
            let size = geometry["size"] as? String ?? "farthest_corner", closest = size.hasPrefix("closest")
            rx = closest ? xs.min()! : xs.max()!; ry = closest ? ys.min()! : ys.max()!
            if geometry["circle"] as? Bool == true {
                let distances = size.hasSuffix("corner") ? xs.flatMap { x in ys.map { hypot(x, $0) } } : xs + ys
                rx = closest ? distances.min()! : distances.max()!; ry = rx
            } else if size.hasSuffix("corner") { rx *= sqrt(2); ry *= sqrt(2) }
            length = rx
        }
        let resolved = stops(length: kind == "conic" ? 0 : length)
        if kind == "conic" {
            let radius = hypot(box.width, box.height) + hypot(origin.x - box.midX, origin.y - box.midY)
            // Bounded angular tessellation; premultiplied colors also handle hard stops.
            var cuts = (0...720).map { CGFloat($0) / 720 }
            if !repeating { cuts += resolved.map { min(1, max(0, $0.0)) } }
            cuts.sort()
            for i in 1..<cuts.count where cuts[i] > cuts[i - 1] {
                let a = cuts[i - 1], b = cuts[i]
                context.move(to: origin)
                context.addArc(center: origin, radius: radius, startAngle: a * 2 * .pi + angle - .pi / 2,
                               endAngle: b * 2 * .pi + angle - .pi / 2, clockwise: false)
                context.closePath(); context.setFillColor(sample((a + b) / 2, resolved).cgColor); context.fillPath()
            }
            return
        }
        var reach: CGFloat = 1
        if kind == "radial", rx > 0, ry > 0 {
            reach = max(1, [box.minX, box.maxX].flatMap { x in [box.minY, box.maxY].map { y in hypot((x-origin.x)/rx, (y-origin.y)/ry) } }.max() ?? 1)
        }
        var expanded = resolved
        if repeating {
            let period = resolved.last!.0 - resolved.first!.0
            if period <= 0.0001 { context.setFillColor(resolved.last!.1.cgColor); context.fill(box); return }
            let low = Int(floor(-resolved.last!.0 / period)), high = Int(ceil((reach - resolved.first!.0) / period))
            if high - low > 16384 / resolved.count {
                context.setFillColor(sample(reach / 2, resolved).cgColor); context.fill(box); return
            }
            expanded = (low...high).flatMap { n in resolved.map { ($0.0 + CGFloat(n) * period, $0.1) } }
        }
        var offsets: [CGFloat] = [0]
        var colors: [CGColor] = [sample(0, resolved).cgColor]
        for stop in expanded where stop.0 >= 0 && stop.0 <= reach { offsets.append(stop.0 / reach); colors.append(stop.1.cgColor) }
        offsets.append(1); colors.append(sample(reach, resolved).cgColor)
        guard let gradient = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: colors as CFArray, locations: offsets) else { return }
        if kind == "radial" {
            guard rx > 0, ry > 0 else { context.setFillColor(resolved.last!.1.cgColor); context.fill(box); return }
            context.saveGState(); context.translateBy(x: origin.x, y: origin.y); context.scaleBy(x: rx, y: ry)
            context.drawRadialGradient(gradient, startCenter: .zero, startRadius: 0, endCenter: .zero, endRadius: reach, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation]); context.restoreGState()
        } else {
            let a = CGPoint(x: box.midX - dx * length / 2, y: box.midY - dy * length / 2)
            let b = CGPoint(x: box.midX + dx * length / 2, y: box.midY + dy * length / 2)
            context.drawLinearGradient(gradient, start: a, end: b, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
        }
    }
}

final class JadixGradientMaskLayer: CALayer {
    var gradients: [JadixGradient] = []
    override func draw(in context: CGContext) { for gradient in gradients.reversed() { gradient.draw(in: context, box: bounds) } }
}

#if !os(iOS)
@MainActor
final class JadixCursorView: NSView {
    var cursor = NSCursor.arrow
    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func resetCursorRects() { addCursorRect(bounds, cursor: cursor) }
    static func resolve(_ name: String) -> NSCursor {
        switch name {
        case "pointer": return .pointingHand; case "text": return .iBeam; case "crosshair": return .crosshair
        case "move", "grab": return .openHand; case "grabbing": return .closedHand; case "not_allowed": return .operationNotAllowed
        case "zoom_in", "zoom_out":
            let selector = NSSelectorFromString(name == "zoom_in" ? "zoomInCursor" : "zoomOutCursor")
            if let meta = object_getClass(NSCursor.self), class_respondsToSelector(meta, selector),
               let value = (NSCursor.self as AnyObject).perform(selector)?.takeUnretainedValue() as? NSCursor { return value }
            return zoomCursor(name == "zoom_in")
        case "resize_horizontal": return .resizeLeftRight; case "resize_vertical": return .resizeUpDown
        case "none": return NSCursor(image: NSImage(size: NSSize(width: 1, height: 1)), hotSpot: .zero)
        case "resize_north_east_south_west", "resize_north_west_south_east":
            let image = NSImage(size: NSSize(width: 20, height: 20), flipped: false) { _ in
                let path = NSBezierPath(); path.lineWidth = 2
                let rising = name == "resize_north_east_south_west"
                let a = NSPoint(x: 3, y: rising ? 3 : 17), b = NSPoint(x: 17, y: rising ? 17 : 3)
                path.move(to: a); path.line(to: b)
                path.move(to: NSPoint(x: 3, y: rising ? 9 : 11)); path.line(to: a); path.line(to: NSPoint(x: 9, y: a.y))
                path.move(to: NSPoint(x: 11, y: b.y)); path.line(to: b); path.line(to: NSPoint(x: 17, y: rising ? 11 : 9))
                JadixColor.labelColor.setStroke(); path.stroke(); return true
            }
            return NSCursor(image: image, hotSpot: NSPoint(x: 10, y: 10))
        default: return .arrow
        }
    }
    private static func zoomCursor(_ inward: Bool) -> NSCursor {
        let image = NSImage(size: NSSize(width: 20, height: 20), flipped: false) { _ in
            let path = NSBezierPath(ovalIn: NSRect(x: 2, y: 7, width: 11, height: 11)); path.lineWidth = 1.5
            path.move(to: NSPoint(x: 12, y: 8)); path.line(to: NSPoint(x: 18, y: 2))
            path.move(to: NSPoint(x: 4.5, y: 12.5)); path.line(to: NSPoint(x: 10.5, y: 12.5))
            if inward { path.move(to: NSPoint(x: 7.5, y: 9.5)); path.line(to: NSPoint(x: 7.5, y: 15.5)) }
            JadixColor.labelColor.setStroke(); path.stroke(); return true
        }
        return NSCursor(image: image, hotSpot: NSPoint(x: 7.5, y: 7.5))
    }
}

#endif

struct JadixEffectFilters {
    static func interpolate(_ a: [[String: Any]], _ b: [[String: Any]], _ t: CGFloat) -> [[String: Any]] {
        guard a.count == b.count else { return t < 0.5 ? a : b }
        return zip(a, b).map { a, b in
            guard a["kind"] as? String == b["kind"] as? String else { return t < 0.5 ? a : b }
            var result = t < 0.5 ? a : b
            if let x = a["value"] as? Double, let y = b["value"] as? Double { result["value"] = x + (y - x) * Double(t) }
            else if let x = a["value"] as? [String: Any], let y = b["value"] as? [String: Any] {
                var value = t < 0.5 ? x : y
                for key in ["blur", "spread"] { if let from = x[key] as? Double, let to = y[key] as? Double { value[key] = from + (to - from) * Double(t) } }
                if let from = x["offset"] as? [Double], let to = y["offset"] as? [Double] { value["offset"] = zip(from, to).map { $0 + ($1 - $0) * Double(t) } }
                if let from = JadixVisualStyle.color(x["color"]), let to = JadixVisualStyle.color(y["color"]), let c = mixColor(from, to, t).usingColorSpace(.sRGB) {
                    value["color"] = [c.redComponent, c.greenComponent, c.blueComponent, c.alphaComponent].map { Double($0 * 255) }
                }
                result["value"] = value
            }
            return result
        }
    }
    static func make(_ values: [[String: Any]]) -> [CIFilter] {
        values.compactMap { item in
            let kind = item["kind"] as? String ?? "", raw = item["value"] as? Double ?? 0
            // Spring easing may overshoot; keep CSS nonnegative and unit-interval filters valid.
            let x = kind == "hue_rotate" ? raw : ["grayscale", "invert", "opacity", "sepia"].contains(kind)
                ? min(1, max(0, raw)) : max(0, raw)
            func filter(_ name: String, _ values: [String: Any]) -> CIFilter? { CIFilter(name: name, parameters: values) }
            func matrix(_ rows: [[Double]], bias: [Double] = [0, 0, 0, 0]) -> CIFilter? {
                filter("CIColorMatrix", ["inputRVector": CIVector(values: rows[0].map { CGFloat($0) }, count: 4), "inputGVector": CIVector(values: rows[1].map { CGFloat($0) }, count: 4),
                                         "inputBVector": CIVector(values: rows[2].map { CGFloat($0) }, count: 4), "inputAVector": CIVector(values: rows[3].map { CGFloat($0) }, count: 4), "inputBiasVector": CIVector(values: bias.map { CGFloat($0) }, count: 4)])
            }
            switch kind {
            case "blur": return filter("CIGaussianBlur", ["inputRadius": x])
            case "brightness": return matrix([[x,0,0,0],[0,x,0,0],[0,0,x,0],[0,0,0,1]])
            case "contrast": return filter("CIColorControls", ["inputContrast": x])
            case "saturate": return filter("CIColorControls", ["inputSaturation": x])
            case "grayscale": return filter("CIColorControls", ["inputSaturation": 1 - x])
            case "hue_rotate": return filter("CIHueAdjust", ["inputAngle": x])
            case "invert": return matrix([[1-2*x,0,0,0],[0,1-2*x,0,0],[0,0,1-2*x,0],[0,0,0,1]], bias: [x,x,x,0])
            case "opacity": return matrix([[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,x]])
            case "sepia": return filter("CISepiaTone", ["inputIntensity": x])
            case "drop_shadow":
                guard let value = item["value"] as? [String: Any] else { return nil }
                let shadow = JadixDropShadowFilter()
                shadow.color = JadixVisualStyle.color(value["color"]) ?? .clear
                let rawOffset = value["offset"] as? [Double] ?? []
                let offset = rawOffset.count == 2 ? rawOffset : [0, 0]
                shadow.offset = CGPoint(x: offset[0], y: -offset[1])
                shadow.blur = max(0, value["blur"] as? Double ?? 0)
                shadow.spread = value["spread"] as? Double ?? 0
                return shadow
            default: return nil
            }
        }
    }
    static func blend(_ name: String) -> String? {
        let names = ["multiply":"multiplyBlendMode", "screen":"screenBlendMode", "overlay":"overlayBlendMode", "darken":"darkenBlendMode", "lighten":"lightenBlendMode", "color_dodge":"colorDodgeBlendMode", "color_burn":"colorBurnBlendMode", "hard_light":"hardLightBlendMode", "soft_light":"softLightBlendMode", "difference":"differenceBlendMode", "exclusion":"exclusionBlendMode", "hue":"hueBlendMode", "saturation":"saturationBlendMode", "color":"colorBlendMode", "luminosity":"luminosityBlendMode", "plus_darker":"plusDarkerBlendMode", "plus_lighter":"plusLighterBlendMode"]
        return names[name]
    }
    static func composite(_ foreground: CIImage, over background: CIImage, mode: String) -> CIImage {
        let names = ["multiply":"CIMultiplyBlendMode", "screen":"CIScreenBlendMode", "overlay":"CIOverlayBlendMode", "darken":"CIDarkenBlendMode", "lighten":"CILightenBlendMode", "color_dodge":"CIColorDodgeBlendMode", "color_burn":"CIColorBurnBlendMode", "hard_light":"CIHardLightBlendMode", "soft_light":"CISoftLightBlendMode", "difference":"CIDifferenceBlendMode", "exclusion":"CIExclusionBlendMode", "hue":"CIHueBlendMode", "saturation":"CISaturationBlendMode", "color":"CIColorBlendMode", "luminosity":"CILuminosityBlendMode", "plus_darker":"CILinearBurnBlendMode", "plus_lighter":"CIAdditionCompositing"]
        guard let name = names[mode] else { return foreground.composited(over: background) }
        return foreground.applyingFilter(name, parameters: [kCIInputBackgroundImageKey: background])
    }
    static func filtered(_ input: CIImage, values: [[String: Any]], scale: CGFloat = 1) -> CIImage {
        var result = input
        for filter in make(values) {
            if filter.name == "CIGaussianBlur", let radius = filter.value(forKey: kCIInputRadiusKey) as? NSNumber { filter.setValue(radius.doubleValue * Double(scale), forKey: kCIInputRadiusKey) }
            if let shadow = filter as? JadixDropShadowFilter {
                shadow.blur *= Double(scale); shadow.spread *= Double(scale)
                shadow.offset = CGPoint(x: shadow.offset.x * scale, y: shadow.offset.y * scale)
            }
            filter.setValue(result, forKey: kCIInputImageKey)
            result = filter.outputImage ?? result
        }
        return result
    }
#if !os(iOS)
    static func material(_ name: String) -> NSVisualEffectView.Material {
        switch name {
        case "titlebar": return .titlebar; case "selection": return .selection; case "menu": return .menu
        case "sidebar": return .sidebar; case "header_view": return .headerView; case "sheet": return .sheet
        case "window_background": return .windowBackground; case "hud_window": return .hudWindow
        case "full_screen_ui": return .fullScreenUI; case "tool_tip": return .toolTip
        case "content_background": return .contentBackground; case "under_window_background": return .underWindowBackground
        case "under_page_background": return .underPageBackground; default: return .popover
        }
    }
#endif
}

#if !os(iOS)
// Decoration must never become the hit target of the decorated control.
@MainActor
final class JadixMaterialView: NSVisualEffectView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

#endif

// Operates on the composed content alpha, preserving filter order. A box shadow
// cannot substitute for this on text, transparent images or masked descendants.
final class JadixDropShadowFilter: CIFilter {
    @objc dynamic var inputImage: CIImage?
    var color: JadixColor = .clear
    var offset: CGPoint = .zero
    var blur: Double = 0, spread: Double = 0
    override var inputKeys: [String] { [kCIInputImageKey] }
    override var outputImage: CIImage? {
        guard let inputImage, let color = color.usingColorSpace(.sRGB) else { return inputImage }
        var alpha = inputImage
        if spread != 0 {
            alpha = alpha.applyingFilter(spread > 0 ? "CIMorphologyMaximum" : "CIMorphologyMinimum",
                                         parameters: [kCIInputRadiusKey: abs(spread)])
        }
        if blur > 0 { alpha = alpha.applyingFilter("CIGaussianBlur", parameters: [kCIInputRadiusKey: blur]) }
        let shadow = alpha.applyingFilter("CIColorMatrix", parameters: [
            "inputRVector": CIVector(x: 0, y: 0, z: 0, w: 0),
            "inputGVector": CIVector(x: 0, y: 0, z: 0, w: 0),
            "inputBVector": CIVector(x: 0, y: 0, z: 0, w: 0),
            "inputAVector": CIVector(x: 0, y: 0, z: 0, w: color.alphaComponent),
            "inputBiasVector": CIVector(x: color.redComponent, y: color.greenComponent, z: color.blueComponent, w: 0)
        ]).transformed(by: CGAffineTransform(translationX: offset.x, y: offset.y))
        return inputImage.composited(over: shadow)
    }
}

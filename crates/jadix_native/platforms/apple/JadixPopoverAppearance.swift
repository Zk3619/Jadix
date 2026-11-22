import Foundation
import CoreGraphics

public enum JadixPopoverEvent {
    public enum DismissReason { case escape, outsidePress }

    public struct Position: Equatable {
        public let frame: CGRect
        public let placement: String
        public let availableSize: CGSize
    }

    case opened
    case closed
    case positionChanged(Position?)
    case dismissRequested(DismissReason, allowed: Bool)
}

struct JadixPopoverAppearance {
    var style: [String: Any]?
    init(style: [String: Any]? = nil) { self.style = style }
    func resolved(over visible: JadixVisualStyle) -> JadixVisualStyle {
        if let full = style?["view"] as? [String: Any] {
            return JadixVisualStyle(full, background: visible.background)
        }
        var result = visible
        result.opacity = CGFloat(style?["opacity"] as? Double ?? 0)
        let scale = CGFloat(style?["scale"] as? Double ?? 1)
        result.matrix[0] *= scale; result.matrix[5] *= scale
        if let offset = style?["offset"] as? [Double], offset.count == 2 {
            result.matrix[12] += CGFloat(offset[0]); result.matrix[13] += CGFloat(offset[1])
        }
        if let color = JadixVisualStyle.color(style?["background"]) { result.background = color }
        if let radius = style?["corner_radius"] as? Double {
            result.corners = Array(repeating: CGFloat(radius), count: 4); result.clip = true
        }
        return result
    }
}

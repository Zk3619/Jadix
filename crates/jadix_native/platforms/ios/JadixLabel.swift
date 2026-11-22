import UIKit

/// TextKit owns wrapping, bidi, shaping and truncation; shared appearance owns
/// authored alignment and decorations. The UILabel remains the accessibility leaf.
@MainActor final class JadixLabel: UILabel {
    private func layout(_ size: CGSize) -> (NSTextStorage, NSLayoutManager, NSTextContainer, CGRect) {
        let source = attributedText ?? NSAttributedString(string: text ?? "", attributes: [.font: font ?? UIFont.systemFont(ofSize: 14), .foregroundColor: textColor ?? UIColor.label])
        let storage = NSTextStorage(attributedString: source)
        let manager = NSLayoutManager(), container = NSTextContainer(size: size)
        container.lineFragmentPadding = 0; container.maximumNumberOfLines = numberOfLines
        container.lineBreakMode = lineBreakMode
        storage.addLayoutManager(manager); manager.addTextContainer(container)
        manager.ensureLayout(for: container)
        return (storage, manager, container, manager.usedRect(for: container))
    }
    override func sizeThatFits(_ size: CGSize) -> CGSize {
        let (_, _, _, rect) = layout(CGSize(width: max(0, size.width), height: max(0, size.height)))
        return CGSize(width: ceil(rect.maxX), height: ceil(rect.maxY))
    }
    func firstBaseline(in size: CGSize) -> Double {
        let (storage, manager, container, used) = layout(size)
        let glyphs = manager.glyphRange(for: container)
        guard storage.length > 0, glyphs.length > 0 else { return .nan }
        let appearance = JadixTextAppearance(storage.attribute(.jadixTextAppearance, at: 0, effectiveRange: nil))
        let line = manager.lineFragmentRect(forGlyphAt: glyphs.location, effectiveRange: nil)
        let baseline = line.minY + manager.location(forGlyphAt: glyphs.location).y
            + appearance.verticalOffset(available: size.height, used: used.maxY)
        return baseline.isFinite && baseline > 0 && baseline <= size.height ? Double(baseline) : .nan
    }
    override func drawText(in rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext() else { return }
        let (storage, manager, container, used) = layout(rect.size)
        guard storage.length > 0 else { return }
        let source = NSAttributedString(attributedString: storage)
        let appearance = JadixTextAppearance(source.attribute(.jadixTextAppearance, at: 0, effectiveRange: nil))
        let origin = CGPoint(x: rect.minX, y: rect.minY + appearance.verticalOffset(available: rect.height, used: used.maxY))
        // Suppress native strokes so thickness, offsets, overline and wave style
        // use the same contract as the desktop renderer, including inline spans.
        storage.removeAttribute(.underlineStyle, range: NSRange(location: 0, length: storage.length))
        storage.removeAttribute(.strikethroughStyle, range: NSRange(location: 0, length: storage.length))
        let glyphs = manager.glyphRange(for: container)
        context.saveGState(); defer { context.restoreGState() }
        context.clip(to: rect)
        manager.drawBackground(forGlyphRange: glyphs, at: origin)
        manager.drawGlyphs(forGlyphRange: glyphs, at: origin)
        source.enumerateAttributes(in: NSRange(location: 0, length: source.length)) { attributes, range, _ in
            let values = attributes[.jadixTextAppearance] as? [String: Any] ?? [:]
            let underline = (attributes[.underlineStyle] as? Int ?? 0) != 0
            let strike = (attributes[.strikethroughStyle] as? Int ?? 0) != 0
            let overline = values["overline"] as? Bool == true
            guard underline || strike || overline else { return }
            let font = attributes[.font] as? UIFont ?? self.font ?? UIFont.systemFont(ofSize: 14)
            let color = JadixVisualStyle.color(values["decoration_color"]) ?? attributes[.foregroundColor] as? UIColor ?? self.textColor ?? .label
            let thickness = CGFloat(values["decoration_thickness"] as? Double ?? Double(max(1, font.ascender / 14)))
            let style = values["decoration_style"] as? String ?? "solid"
            let span = NSIntersectionRange(manager.glyphRange(forCharacterRange: range, actualCharacterRange: nil), glyphs)
            guard span.length > 0 else { return }
            manager.enumerateLineFragments(forGlyphRange: span) { lineRect, _, _, lineRange, _ in
                let part = NSIntersectionRange(span, lineRange)
                guard part.length > 0 else { return }
                let bounds = manager.boundingRect(forGlyphRange: part, in: container)
                let baseline = origin.y + lineRect.minY + manager.location(forGlyphAt: part.location).y
                func stroke(_ y: CGFloat) {
                    JadixTextAppearance.drawDecoration(context, from: origin.x + bounds.minX, width: bounds.width,
                        y: y, thickness: thickness, style: style, color: color)
                }
                if underline { stroke(baseline + CGFloat(values["underline_offset"] as? Double ?? Double(max(1, -font.descender / 2)))) }
                if strike { stroke(baseline - font.ascender * 0.35) }
                if overline { stroke(baseline - font.ascender) }
            }
        }
    }
}

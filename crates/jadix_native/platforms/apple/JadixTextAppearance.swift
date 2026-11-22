import CoreText
#if os(iOS)
import UIKit
typealias JadixFont = UIFont
typealias JadixFontDescriptor = UIFontDescriptor
#else
import AppKit
typealias JadixFont = NSFont
typealias JadixFontDescriptor = NSFontDescriptor
#endif

// Shared TextAppearance is independent of the AppKit control or the authoring entry point.
struct JadixTextAppearance {
    let values: [String: Any]
    init(_ value: Any?) { values = value as? [String: Any] ?? [:] }
    var direction: NSWritingDirection {
        switch values["direction"] as? String {
        case "ltr": return .leftToRight
        case "rtl": return .rightToLeft
        default: return .natural
        }
    }
    static func resolvedFont(families: String, size: CGFloat, weight numericWeight: Double) -> JadixFont {
        let weight: JadixFont.Weight
        switch numericWeight {
        case ..<150: weight = .ultraLight
        case ..<250: weight = .thin
        case ..<350: weight = .light
        case ..<450: weight = .regular
        case ..<550: weight = .medium
        case ..<650: weight = .semibold
        case ..<750: weight = .bold
        case ..<850: weight = .heavy
        default: weight = .black
        }
        let system = JadixFont.systemFont(ofSize: size, weight: weight)
        let candidates = fontFamilies(in: families).compactMap { name -> JadixFont? in
            let design: JadixFontDescriptor.SystemDesign
            switch name.lowercased() {
            case "system-ui", "ui-sans-serif", "sans-serif":
                return system
            case "monospace", "ui-monospace":
                return JadixFont.monospacedSystemFont(ofSize: size, weight: weight)
            case "serif", "ui-serif":
                design = .serif
            case "ui-rounded":
                design = .rounded
            default:
                let descriptor = JadixFontDescriptor(fontAttributes: [
                    .family: name,
                    .traits: [JadixFontDescriptor.TraitKey.weight: weight.rawValue],
                ])
#if os(iOS)
                let font = UIFont(descriptor: descriptor, size: size)
                if font.familyName.caseInsensitiveCompare(name) == .orderedSame { return font }
#else
                if let font = NSFont(descriptor: descriptor, size: size),
                   font.familyName?.caseInsensitiveCompare(name) == .orderedSame { return font }
#endif
                return JadixFont(name: name, size: size)
            }
            return system.fontDescriptor.withDesign(design)
                .flatMap { JadixFont(descriptor: $0, size: size) }
        }
        guard let primary = candidates.first else { return system }
        guard candidates.count > 1 else { return primary }
        let coreTextFont = CTFontCreateWithName(primary.fontName as CFString, size, nil)
        let systemFallbacks = CTFontCopyDefaultCascadeListForLanguages(coreTextFont, nil) as? [Any] ?? []
        let authoredFallbacks: [Any] = candidates.dropFirst().map(\.fontDescriptor)
        let cascade = authoredFallbacks + systemFallbacks
        let descriptor = primary.fontDescriptor.addingAttributes([.cascadeList: cascade])
#if os(iOS)
        return UIFont(descriptor: descriptor, size: size)
#else
        return NSFont(descriptor: descriptor, size: size) ?? primary
#endif
    }

    private static func fontFamilies(in source: String) -> [String] {
        var families: [String] = []
        var family = ""
        var quote: Character?
        var escaped = false
        for character in source {
            if escaped {
                family.append(character)
                escaped = false
            } else if let delimiter = quote {
                if character == "\\" { escaped = true }
                else if character == delimiter { quote = nil }
                else { family.append(character) }
            } else if (character == "\"" || character == "'"),
                      family.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                quote = character
            } else if character == "," {
                families.append(family)
                family = ""
            } else {
                family.append(character)
            }
        }
        families.append(family)
        return families.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
    }

    func font(_ base: JadixFont) -> JadixFont {
        var font = base
        if values["italic"] as? Bool == true {
#if os(iOS)
            if let descriptor = font.fontDescriptor.withSymbolicTraits(.traitItalic) { font = UIFont(descriptor: descriptor, size: font.pointSize) }
#else
            font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
#endif
        }
        var attributes: [JadixFontDescriptor.AttributeName: Any] = [:]
        if let cascade = base.fontDescriptor.object(forKey: .cascadeList) { attributes[.cascadeList] = cascade }
        var features: [[JadixFontDescriptor.FeatureKey: Int]] = []
        for variant in values["variants"] as? [String] ?? [] {
            let feature: (Int, Int)?
            switch variant {
            case "small_caps": feature = (Int(kLowerCaseType), Int(kLowerCaseSmallCapsSelector))
            case "oldstyle_numbers": feature = (Int(kNumberCaseType), Int(kLowerCaseNumbersSelector))
            case "lining_numbers": feature = (Int(kNumberCaseType), Int(kUpperCaseNumbersSelector))
            case "tabular_numbers": feature = (Int(kNumberSpacingType), Int(kMonospacedNumbersSelector))
            case "proportional_numbers": feature = (Int(kNumberSpacingType), Int(kProportionalNumbersSelector))
            default: feature = nil
            }
            if let feature { features.append([.typeIdentifier: feature.0, .selectorIdentifier: feature.1]) }
        }
        if !features.isEmpty { attributes[.featureSettings] = features }
#if os(iOS)
        return UIFont(descriptor: font.fontDescriptor.addingAttributes(attributes), size: font.pointSize)
#else
        return NSFont(descriptor: font.fontDescriptor.addingAttributes(attributes), size: font.pointSize) ?? font
#endif
    }
    func paragraph(lineBreak: NSLineBreakMode = .byWordWrapping) -> NSMutableParagraphStyle {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = lineBreak
        paragraph.baseWritingDirection = direction
        switch values["align"] as? String {
        case "left": paragraph.alignment = .left
        case "right": paragraph.alignment = .right
        case "center": paragraph.alignment = .center
        case "justify": paragraph.alignment = .justified
        case "end": paragraph.alignment = direction == .rightToLeft ? .left : .right
        default: paragraph.alignment = direction == .rightToLeft ? .right : .natural
        }
        if let height = values["line_height"] as? Double {
            paragraph.minimumLineHeight = CGFloat(height); paragraph.maximumLineHeight = CGFloat(height)
        } else if let multiple = values["line_height_multiple"] as? Double {
            paragraph.lineHeightMultiple = CGFloat(multiple)
        }
        return paragraph
    }
    func attributes(font: JadixFont, foreground: JadixColor, lineBreak: NSLineBreakMode = .byWordWrapping) -> [NSAttributedString.Key: Any] {
        var attributes: [NSAttributedString.Key: Any] = [
            .font: font, .foregroundColor: foreground, .paragraphStyle: paragraph(lineBreak: lineBreak),
            .kern: values["letter_spacing"] as? Double ?? 0,
            .jadixTextAppearance: values,
        ]
        var decoration = NSUnderlineStyle.single.rawValue
        switch values["decoration_style"] as? String {
        case "double": decoration = NSUnderlineStyle.double.rawValue
        case "dotted": decoration |= NSUnderlineStyle.patternDot.rawValue
        case "dashed": decoration |= NSUnderlineStyle.patternDash.rawValue
        default: break
        }
        if values["underline"] as? Bool == true { attributes[.underlineStyle] = decoration }
        if values["strikethrough"] as? Bool == true { attributes[.strikethroughStyle] = decoration }
        if let color = JadixVisualStyle.color(values["decoration_color"]) {
            attributes[.underlineColor] = color; attributes[.strikethroughColor] = color
        }
        if let value = values["shadow"] as? [String: Any] {
            let shadow = NSShadow()
            let offset = value["offset"] as? [Double] ?? [0, 0]
            #if os(iOS)
            shadow.shadowOffset = CGSize(width: offset.count == 2 ? offset[0] : 0, height: offset.count == 2 ? offset[1] : 0)
#else
            shadow.shadowOffset = NSSize(width: offset.count == 2 ? offset[0] : 0, height: offset.count == 2 ? -offset[1] : 0)
#endif
            shadow.shadowBlurRadius = CGFloat(value["blur"] as? Double ?? 0)
            shadow.shadowColor = JadixVisualStyle.color(value["color"])
            attributes[.shadow] = shadow
        }
        return attributes
    }
    // Apply a display transform after inline source ranges have been resolved.
    func display(_ source: NSAttributedString) -> NSAttributedString {
        let transform = values["transform"] as? String ?? "none"
        guard transform != "none", source.length > 0 else { return source }
        let result = NSMutableAttributedString(string: "")
        var offset = 0
        var wordStart = true
        for character in source.string {
            let text = String(character)
            let alphanumeric = text.unicodeScalars.contains { CharacterSet.alphanumerics.contains($0) }
            let display: String
            switch transform {
            case "uppercase": display = text.uppercased()
            case "lowercase": display = text.lowercased()
            case "capitalize": display = wordStart && alphanumeric ? text.uppercased() : text
            default: display = text
            }
            result.append(NSAttributedString(string: display, attributes: source.attributes(at: offset, effectiveRange: nil)))
            offset += text.utf16.count
            wordStart = !alphanumeric && text != "'" && text != "’"
        }
        return result
    }
#if os(iOS)
    func applyEditor(_ editor: UITextView, font: UIFont, foreground: UIColor) {
        guard editor.markedTextRange == nil else { return }
        let attributes = attributes(font: font, foreground: foreground)
        let selection = editor.selectedRange
        editor.typingAttributes = attributes
        if editor.textStorage.length > 0 { editor.textStorage.setAttributes(attributes, range: NSRange(location: 0, length: editor.textStorage.length)) }
        editor.selectedRange = selection
        editor.tintColor = JadixVisualStyle.color(values["caret_color"]) ?? .systemBlue
        editor.textAlignment = paragraph().alignment
    }
#else
    func applyEditor(_ editor: NSTextView, font: JadixFont, foreground: JadixColor) {
        let attributes = attributes(font: font, foreground: foreground)
        guard !editor.hasMarkedText() else { return }
        editor.typingAttributes = attributes
        editor.defaultParagraphStyle = paragraph()
        editor.alignment = paragraph().alignment
        editor.baseWritingDirection = direction
        editor.insertionPointColor = JadixVisualStyle.color(values["caret_color"]) ?? .textColor
        editor.selectedTextAttributes = [.backgroundColor: JadixVisualStyle.color(values["selection_color"]) ?? JadixColor.selectedTextBackgroundColor,
                                         .foregroundColor: JadixColor.selectedTextColor]
        // Attribute-only updates preserve selection, undo and IME marked text.
        if let storage = editor.textStorage, storage.length > 0 {
            let selection = editor.selectedRanges
            storage.setAttributes(attributes, range: NSRange(location: 0, length: storage.length))
            editor.selectedRanges = selection
        }
    }
#endif
    func verticalOffset(available: CGFloat, used: CGFloat) -> CGFloat {
        let spare = max(0, available - used)
        if let fraction = values["vertical_align_fraction"] as? Double { return spare * CGFloat(fraction) }
        switch values["vertical_align"] as? String {
        case "center": return spare / 2
        case "bottom": return spare
        default: return 0
        }
    }
    static func drawDecoration(_ context: CGContext, from x: CGFloat, width: CGFloat, y: CGFloat,
                               thickness: CGFloat, style: String, color: JadixColor) {
        guard width > 0, thickness > 0 else { return }
        context.saveGState(); defer { context.restoreGState() }
        context.setStrokeColor(color.cgColor); context.setLineWidth(thickness)
        if style == "dotted" { context.setLineDash(phase: 0, lengths: [thickness, thickness * 2]) }
        if style == "dashed" { context.setLineDash(phase: 0, lengths: [thickness * 4, thickness * 2]) }
        if style == "wavy" {
            let amplitude = max(1, thickness), step = max(2, thickness * 2)
            context.move(to: CGPoint(x: x, y: y))
            var position: CGFloat = 0
            var sign: CGFloat = 1
            while position < width {
                let next = min(width, position + step)
                context.addQuadCurve(to: CGPoint(x: x + next, y: y),
                                     control: CGPoint(x: x + (position + next) / 2, y: y + amplitude * sign))
                position = next; sign = -sign
            }
        } else {
            context.move(to: CGPoint(x: x, y: y)); context.addLine(to: CGPoint(x: x + width, y: y))
            if style == "double" {
                context.move(to: CGPoint(x: x, y: y - thickness * 2))
                context.addLine(to: CGPoint(x: x + width, y: y - thickness * 2))
            }
        }
        context.strokePath()
    }
}

extension NSAttributedString.Key {
    static let jadixTextAppearance = NSAttributedString.Key("JadixTextAppearance")
    static let jadixUnderline = NSAttributedString.Key("JadixUnderline")
}


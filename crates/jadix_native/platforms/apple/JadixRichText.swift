#if os(iOS)
import UIKit
#else
import AppKit
#endif

enum JadixRichText {
    struct Image: Decodable {
        let source: String
        let alt: String
        let width: Double?
        let height: Double?
        let html: String
    }
    struct Plan: Decodable { let html: String; let images: [Image] }
    static func plan(_ html: String) -> Plan? {
        guard html.utf8.count <= 1_048_576, !html.contains("\0"),
              let pointer = html.withCString({ jadix_native_rich_html_plan($0) }) else { return nil }
        defer { jadix_native_string_free(pointer) }
        return try? JSONDecoder().decode(Plan.self, from: Data(String(cString: pointer).utf8))
    }
    static func imageSize(_ image: Image, natural: CGSize = .zero) -> CGSize {
        guard let pointer = jadix_native_rich_image_size(image.width ?? -1, image.height ?? -1,
            Double(natural.width), Double(natural.height)) else { return .zero }
        defer { jadix_native_string_free(pointer) }
        guard let size = try? JSONDecoder().decode([Double].self, from: Data(String(cString: pointer).utf8)),
              size.count == 2 else { return .zero }
        return CGSize(width: size[0], height: size[1])
    }
    static func sanitize(_ html: String) -> String? {
        guard html.utf8.count <= 1_048_576, !html.contains("\0"),
              let pointer = html.withCString({ jadix_native_rich_html($0) }) else { return nil }
        defer { jadix_native_string_free(pointer) }
        return String(cString: pointer)
    }
    static func attributed(_ html: String) -> NSAttributedString? {
        guard let safe = sanitize(html) else { return nil }
        return try? NSAttributedString(data: Data(safe.utf8), options: [.documentType: NSAttributedString.DocumentType.html,
            .characterEncoding: String.Encoding.utf8.rawValue], documentAttributes: nil)
    }
    static func html(_ text: NSAttributedString) -> String? {
        guard let data = try? text.data(from: NSRange(location: 0, length: text.length), documentAttributes: [.documentType: NSAttributedString.DocumentType.html]),
              data.count <= 1_048_576, let html = String(data: data, encoding: .utf8) else { return nil }
        return sanitize(html)
    }
    static func selection(_ range: NSRange, in text: String) -> NSRange {
        let length = (text as NSString).length
        guard range.location != NSNotFound else { return NSRange(location: length, length: 0) }
        let start = min(length, range.location), count = min(length - start, range.length)
        return selection([start, start + count], in: text)
    }
    static func selection(_ range: [Int], in text: String) -> NSRange {
        let text = text as NSString
        var start = min(text.length, max(0, range.first ?? 0)), end = min(text.length, max(0, range.last ?? 0))
        end = max(start, end)
        func trailingSurrogate(_ i: Int) -> Bool { i > 0 && i < text.length && (0xDC00...0xDFFF).contains(Int(text.character(at: i))) }
        if trailingSurrogate(start) { start -= 1 }
        if trailingSurrogate(end) { end += 1 }
        return NSRange(location: start, length: end - start)
    }
}

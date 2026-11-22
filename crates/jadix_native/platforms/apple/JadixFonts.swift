import Foundation
import CoreText
import CryptoKit

extension Notification.Name { static let jadixFontsChanged = Notification.Name("JadixFontsChanged") }

/// Process-scoped registration is shared by every native root. Identical data
/// is registered once, and a successful new font invalidates existing roots.
@MainActor
public enum JadixFonts {
    private static var registered: [String: [String]] = [:]
    private static var retainedBytes = 0
    public static func register(_ data: Data) throws -> [String] {
        let key = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        if let names = registered[key] { return names }
        guard !data.isEmpty, data.count <= 16 * 1024 * 1024,
              registered.count < 128, retainedBytes + data.count <= 32 * 1024 * 1024,
              let descriptors = CTFontManagerCreateFontDescriptorsFromData(data as CFData) as? [CTFontDescriptor], !descriptors.isEmpty else {
            throw NSError(domain: "JadixFonts", code: 1, userInfo: [NSLocalizedDescriptionKey: "Invalid font or registration budget exhausted"])
        }
        var added: [CGFont] = []
        for descriptor in descriptors {
            let font = CTFontCopyGraphicsFont(CTFontCreateWithFontDescriptor(descriptor, 0, nil), nil)
            var error: Unmanaged<CFError>?
            if CTFontManagerRegisterGraphicsFont(font, &error) {
                added.append(font)
            } else {
                let failure = error?.takeRetainedValue()
                // A font already supplied by the embedding process is usable.
                if failure.map({ CFErrorGetCode($0) }) == CTFontManagerError.alreadyRegistered.rawValue { continue }
                for registered in added { CTFontManagerUnregisterGraphicsFont(registered, nil) }
                if let failure { throw failure as Error }
                throw NSError(domain: "JadixFonts", code: 2)
            }
        }
        let names = descriptors.compactMap { CTFontDescriptorCopyAttribute($0, kCTFontNameAttribute) as? String }
        retainedBytes += data.count; registered[key] = names
        NotificationCenter.default.post(name: .jadixFontsChanged, object: nil)
        return names
    }
    public static func register(contentsOf url: URL) throws -> [String] {
        guard url.isFileURL, let values = try? url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey]),
              values.isRegularFile == true, let size = values.fileSize, size > 0, size <= 16 * 1024 * 1024 else {
            throw NSError(domain: "JadixFonts", code: 1)
        }
        return try register(Data(contentsOf: url, options: .mappedIfSafe))
    }
}

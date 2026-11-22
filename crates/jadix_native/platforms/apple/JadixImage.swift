import Foundation
import CoreGraphics
import CryptoKit
import Darwin
import ImageIO
import UniformTypeIdentifiers

let appleImageQueue = DispatchQueue(label: "jadix.apple.image", qos: .userInitiated)
let appleInlineImagePrefix = "data:image/"
let applePNGPrefix = "data:image/png;base64,"
let appleAssetPrefix = "asset:/"
let appleImageFileLimit = 16 * 1024 * 1024
final class AppleDecodedImage {
    let frames: [CGImage]
    let frameDelays: [Double]
    let originalSize: CGSize
    let sequence: AppleImageSequence?

    init(frames: [CGImage], frameDelays: [Double], originalSize: CGSize, sequence: AppleImageSequence? = nil) {
        self.frames = frames
        self.frameDelays = frameDelays
        self.originalSize = originalSize
        self.sequence = sequence
    }
}

// ImageIO and its source are accessed only on appleImageQueue. The player
// retains the first, current and next frame, independent of the sequence length.
final class AppleImageSequence {
    let input: CGImageSource
    let options: CFDictionary
    let count: Int
    let firstDelay: Double
    init(input: CGImageSource, options: CFDictionary, count: Int, firstDelay: Double) {
        self.input = input; self.options = options; self.count = count; self.firstDelay = firstDelay
    }
    func frame(_ index: Int) -> (CGImage, Double)? {
        guard let properties = CGImageSourceCopyPropertiesAtIndex(input, index, nil) as? [String: Any],
              let width = properties[kCGImagePropertyPixelWidth as String] as? Int,
              let height = properties[kCGImagePropertyPixelHeight as String] as? Int,
              width > 0, height > 0, width <= 16_384, height <= 16_384,
              width * height <= 64 * 1024 * 1024,
              let image = CGImageSourceCreateThumbnailAtIndex(input, index, options),
              image.bytesPerRow * image.height <= 4 * 1024 * 1024 else { return nil }
        return (image, appleAnimationDelay(properties) ?? 0.1)
    }
}

let appleImageCache: NSCache<NSString, AppleDecodedImage> = {
    let cache = NSCache<NSString, AppleDecodedImage>()
    cache.countLimit = 64
    cache.totalCostLimit = 16 * 1024 * 1024
    return cache
}()

func appleInlineImageData(_ source: String) -> Data? {
    guard source.utf8.count <= 1024 * 1024,
          let delimiter = source.range(of: ";base64,") else { return nil }
    return Data(base64Encoded: String(source[delimiter.upperBound...]))
}

func appleImageFile(_ source: String) -> URL? {
    if source.hasPrefix(appleAssetPrefix) {
        let path = String(source.dropFirst(appleAssetPrefix.count))
        guard let resourceURL = Bundle.main.resourceURL else { return nil }
        let resources = resourceURL.resolvingSymlinksInPath()
        let file = resources.appendingPathComponent(path, isDirectory: false).resolvingSymlinksInPath()
        let prefix = resources.path.hasSuffix("/") ? resources.path : resources.path + "/"
        return file.path.hasPrefix(prefix) ? file : nil
    }
    guard source.hasPrefix("file:///"), let file = URL(string: source), file.isFileURL,
          file.host?.isEmpty != false, file.query == nil, file.fragment == nil else { return nil }
    return file
}

func appleImageData(_ source: String) -> Data? {
    if source.hasPrefix(appleInlineImagePrefix) { return appleInlineImageData(source) }
    guard let url = appleImageFile(source), !url.path.utf8.contains(0) else { return nil }
    let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
        guard let path else { return -1 }
        return Darwin.open(path, O_RDONLY | O_NONBLOCK | O_CLOEXEC)
    }
    guard descriptor >= 0 else { return nil }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    var info = stat()
    guard fstat(descriptor, &info) == 0, (Int(info.st_mode) & Int(S_IFMT)) == Int(S_IFREG),
          info.st_size > 0, info.st_size <= off_t(appleImageFileLimit) else { return nil }
    guard let data = try? handle.read(upToCount: appleImageFileLimit + 1),
          data.count <= appleImageFileLimit else { return nil }
    return data
}

struct AppleRemoteImagePath: Sendable {
    let segments: [Data]

    init?(_ path: String, percentEncoded: Bool) {
        guard path.hasPrefix("/") else { return nil }
        let parts = path.dropFirst().split(separator: "/", omittingEmptySubsequences: false)
        var segments: [Data] = []
        for (index, part) in parts.enumerated() {
            if part.isEmpty, index == parts.count - 1 { continue }
            let text = String(part)
            guard let segment = percentEncoded ? text.removingPercentEncoding : text,
                  !segment.isEmpty, segment != ".", segment != "..",
                  segment.utf8.allSatisfy({ $0 >= 32 && $0 != 127 }),
                  segment.rangeOfCharacter(from: CharacterSet(charactersIn: "/\\%;?#")) == nil else {
                return nil
            }
            segments.append(Data(segment.utf8))
        }
        self.segments = segments
    }

    func covers(_ path: Self) -> Bool {
        path.segments.starts(with: segments)
    }
}

public struct JadixRemoteImagePolicy: Sendable {
    private let allowedHosts: Set<String>?
    private let pathPrefixes: [String: [AppleRemoteImagePath]]?
    private let valid: Bool

    public init(hosts: [String]? = nil, pathPrefixes: [String: [String]]? = nil) {
        allowedHosts = hosts.map { Set($0.map { $0.lowercased() }) }
        var parsedPaths: [String: [AppleRemoteImagePath]]?
        var valid = true
        if let pathPrefixes {
            var parsed: [String: [AppleRemoteImagePath]] = [:]
            for (host, paths) in pathPrefixes {
                let name = host.lowercased()
                let prefixes = paths.compactMap { AppleRemoteImagePath($0, percentEncoded: false) }
                if name.isEmpty || parsed[name] != nil || prefixes.count != paths.count {
                    valid = false
                    break
                }
                parsed[name] = prefixes
            }
            parsedPaths = parsed
        } else {
            parsedPaths = nil
        }
        self.pathPrefixes = parsedPaths
        self.valid = valid
    }

    static let application: JadixRemoteImagePolicy = {
        let info = Bundle.main.infoDictionary ?? [:]
        let hosts: [String]? = info["JadixRemoteImageHosts"].map { $0 as? [String] ?? [] }
        let paths: [String: [String]]? = info["JadixRemoteImagePathPrefixes"].map {
            $0 as? [String: [String]] ?? [:]
        }
        return JadixRemoteImagePolicy(hosts: hosts, pathPrefixes: paths)
    }()

    func allows(_ url: URL?) -> URL? {
        guard valid, let url, url.scheme?.lowercased() == "https", let host = url.host, !host.isEmpty,
              url.user == nil, url.password == nil, url.fragment == nil,
              allowedHosts?.contains(host.lowercased()) ?? true else { return nil }
        if let pathPrefixes {
            guard let prefixes = pathPrefixes[host.lowercased()],
                  url.port == nil || url.port == 443,
                  let encoded = URLComponents(url: url, resolvingAgainstBaseURL: true)?.percentEncodedPath,
                  let path = AppleRemoteImagePath(encoded.isEmpty ? "/" : encoded, percentEncoded: true),
                  prefixes.contains(where: { $0.covers(path) }) else { return nil }
        }
        return url
    }
}

final class AppleImageRedirects: NSObject, URLSessionTaskDelegate {
    private let policy: JadixRemoteImagePolicy
    private let lock = NSLock()
    private var redirects: [String] = []
    private var cacheable = true

    init(policy: JadixRemoteImagePolicy) { self.policy = policy }

    func allowedURL(_ url: URL?) -> URL? {
        policy.allows(JadixRemoteImagePolicy.application.allows(url))
    }

    func tracking() -> AppleImageRedirects { AppleImageRedirects(policy: policy) }

    func allowsCaching() -> Bool { lock.lock(); defer { lock.unlock() }; return cacheable }

    func chain(from origin: URL, to final: URL) -> [String] {
        lock.lock(); defer { lock.unlock() }
        return [origin.absoluteString] + redirects + [final.absoluteString]
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        let url = allowedURL(request.url)
        if let url {
            lock.lock()
            redirects.append(url.absoluteString)
            let directives = (response.value(forHTTPHeaderField: "Cache-Control") ?? "").lowercased().split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
            cacheable = cacheable && !directives.contains("no-store") && !directives.contains("private")
                && response.value(forHTTPHeaderField: "Vary") == nil && response.value(forHTTPHeaderField: "Set-Cookie") == nil
            lock.unlock()
        }
        completionHandler(url == nil ? nil : request)
    }
}

// One atomic entry keeps bytes and their authorization provenance inseparable.
// Cache work runs outside the UI actor; roots still recheck every stored hop.
actor AppleImageDiskCache {
    private var budget = 8 * 1024 * 1024, entryLimit = 32
    private var age: TimeInterval = 30
    private var memory: [URL: (Entry, Date)] = [:]
    private var retainedBytes = 0
    func configure(bytes: Int, entries: Int, age: TimeInterval) {
        budget = bytes; entryLimit = entries; self.age = age
        memory.removeAll(); retainedBytes = 0
    }
    private func retain(_ url: URL, _ entry: Entry) {
        if let old = memory.removeValue(forKey: url) { retainedBytes -= old.0.body.count }
        guard budget > 0, entryLimit > 0, age > 0, entry.body.count <= budget else { return }
        while retainedBytes + entry.body.count > budget || memory.count >= entryLimit {
            guard let oldest = memory.min(by: { $0.value.1 < $1.value.1 }) else { break }
            memory.removeValue(forKey: oldest.key); retainedBytes -= oldest.value.0.body.count
        }
        var entry = entry
        entry.expires = min(entry.expires, Date().addingTimeInterval(age))
        memory[url] = (entry, Date()); retainedBytes += entry.body.count
    }
    struct Entry: Codable {
        var chain: [String]
        var body: Data
        var expires: Date
        var etag: String?
        var modified: String?
    }
    private let root: URL? = {
        guard let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else { return nil }
        return caches.appendingPathComponent(Bundle.main.bundleIdentifier ?? "org.jadix.native", isDirectory: true)
            .appendingPathComponent("images-v1", isDirectory: true)
    }()
    private func path(_ url: URL) -> URL? {
        let key = SHA256.hash(data: Data(url.absoluteString.utf8)).map { String(format: "%02x", $0) }.joined()
        return root?.appendingPathComponent(key).appendingPathExtension("json")
    }
    func read(_ url: URL) -> Entry? {
        if let cached = memory[url], cached.0.expires > Date() {
            memory[url] = (cached.0, Date()); return cached.0
        }
        if let old = memory.removeValue(forKey: url) { retainedBytes -= old.0.body.count }
        guard let path = path(url),
              let size = try? path.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey]), size.isRegularFile == true,
              (size.fileSize ?? Int.max) <= 23 * 1024 * 1024,
              let data = try? Data(contentsOf: path), let entry = try? JSONDecoder().decode(Entry.self, from: data),
              entry.body.count <= appleImageFileLimit, entry.chain.first == url.absoluteString,
              entry.chain.count <= 22, !entry.chain.isEmpty else { return nil }
        try? FileManager.default.setAttributes([.modificationDate: Date()], ofItemAtPath: path.path)
        retain(url, entry)
        return entry
    }
    func remove(_ url: URL) {
        if let old = memory.removeValue(forKey: url) { retainedBytes -= old.0.body.count }
        if let path = path(url) { try? FileManager.default.removeItem(at: path) } }
    func store(_ url: URL, body: Data, chain: [String], response: HTTPURLResponse, previous: Entry?) -> Entry? {
        let control = response.value(forHTTPHeaderField: "Cache-Control")?.lowercased() ?? ""
        let directives = control.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
        // Vary responses need a request-header cache key; do not guess one.
        guard !directives.contains("no-store"), !directives.contains("private"),
              response.value(forHTTPHeaderField: "Vary") == nil,
              response.value(forHTTPHeaderField: "Set-Cookie") == nil,
              body.count <= appleImageFileLimit, chain.count <= 22 else { remove(url); return nil }
        var expires = previous?.expires ?? .distantPast
        if let maxAge = directives.first(where: { $0.hasPrefix("max-age=") }),
           let seconds = Double(maxAge.dropFirst(8).trimmingCharacters(in: CharacterSet(charactersIn: "\""))) {
            let age = Double(response.value(forHTTPHeaderField: "Age") ?? "0") ?? 0
            expires = Date().addingTimeInterval(max(0, min(604800, seconds - age)))
        } else if let raw = response.value(forHTTPHeaderField: "Expires") {
            let formatter = DateFormatter()
            formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.timeZone = TimeZone(secondsFromGMT: 0)
            formatter.dateFormat = "EEE, dd MMM yyyy HH:mm:ss zzz"
            expires = formatter.date(from: raw) ?? .distantPast
        }
        // Redirect freshness has its own lifetime. Revalidate redirected entries
        // instead of letting the final response extend an intermediate hop.
        if directives.contains("no-cache") || chain.count > 2 { expires = .distantPast }
        let entry = Entry(chain: chain, body: body, expires: expires,
                          etag: response.value(forHTTPHeaderField: "ETag") ?? previous?.etag,
                          modified: response.value(forHTTPHeaderField: "Last-Modified") ?? previous?.modified)
        retain(url, entry)
        guard let root, let path = path(url), let data = try? JSONEncoder().encode(entry) else { return entry }
        do {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            try data.write(to: path, options: .atomic)
            let keys: Set<URLResourceKey> = [.fileSizeKey, .contentModificationDateKey, .isRegularFileKey]
            let files = try FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: Array(keys))
                .filter { $0.pathExtension == "json" }
                .compactMap { file -> (URL, Int, Date)? in
                    guard let values = try? file.resourceValues(forKeys: keys), values.isRegularFile == true else { return nil }
                    return (file, values.fileSize ?? 0, values.contentModificationDate ?? .distantPast)
                }.sorted { $0.2 < $1.2 }
            var total = files.reduce(0) { $0 + $1.1 }, count = files.count
            for file in files where total > 64 * 1024 * 1024 || count > 256 {
                try? FileManager.default.removeItem(at: file.0); total -= file.1; count -= 1
            }
        } catch { /* A cache failure must not discard a successful download. */ }
        return entry
    }
}

@MainActor
final class AppleRemoteImages {
    private final class Transfer {
        let url: URL
        var callbacks: [UUID: (Data?) -> Void] = [:]
        var progress: [UUID: (Int, Int) -> Void] = [:]
        var task: Task<Void, Never>?

        init(url: URL) { self.url = url }
    }

    private let redirects: AppleImageRedirects
    private let cache = AppleImageDiskCache()
    private let session: URLSession
    func configureCache(bytes: Int, entries: Int, age: TimeInterval) async {
        await cache.configure(bytes: bytes, entries: entries, age: age)
    }

    init(policy: JadixRemoteImagePolicy) {
        let redirects = AppleImageRedirects(policy: policy)
        self.redirects = redirects
        let config = URLSessionConfiguration.ephemeral
        config.httpShouldSetCookies = false
        config.httpCookieStorage = nil
        config.urlCredentialStorage = nil
        // Only the provenance-aware cache may bypass redirect validation.
        config.urlCache = nil
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.timeoutIntervalForRequest = 15
        config.timeoutIntervalForResource = 30
        session = URLSession(configuration: config, delegate: redirects, delegateQueue: nil)
    }
    private var transfers: [URL: Transfer] = [:]
    private var waiting: [URL] = []
    private var active = 0

    func decode(
        _ request: AppleImageRequest, completion: @escaping (AppleDecodedImage?) -> Void,
        networkFinished: @escaping () -> Void = {}, progress: ((Int, Int) -> Void)? = nil
    ) -> (url: URL, token: UUID)? {
        func decodeData(_ data: Data?) {
            appleImageQueue.async {
                let decoded = decodeAppleImage(request, remoteData: data)
                DispatchQueue.main.async { completion(decoded) }
            }
        }
        let transfer = load(URL(string: request.source), progress: progress) { data in
            networkFinished()
            if let data { decodeData(data) }
            else { completion(nil) }
        }
        if transfer == nil { decodeData(nil) }
        return transfer
    }

    func load(_ candidate: URL?, progress: ((Int, Int) -> Void)?,
              completion: @escaping (Data?) -> Void) -> (url: URL, token: UUID)? {
        guard let url = redirects.allowedURL(candidate) else { return nil }
        let token = UUID()
        let transfer: Transfer
        if let existing = transfers[url] {
            transfer = existing
        } else {
            transfer = Transfer(url: url)
            transfers[url] = transfer
            waiting.append(url)
        }
        transfer.callbacks[token] = completion
        transfer.progress[token] = progress
        startWaiting()
        return (url, token)
    }

    func cancel(_ url: URL, token: UUID) {
        guard let transfer = transfers[url] else { return }
        transfer.callbacks.removeValue(forKey: token)
        transfer.progress.removeValue(forKey: token)
        guard transfer.callbacks.isEmpty else { return }
        transfers.removeValue(forKey: url)
        if let task = transfer.task { task.cancel() }
        else { waiting.removeAll { $0 == url } }
    }

    func invalidate() {
        for transfer in transfers.values { transfer.task?.cancel() }
        transfers.removeAll()
        waiting.removeAll()
        session.invalidateAndCancel()
    }

    private func startWaiting() {
        while active < 4, !waiting.isEmpty {
            let url = waiting.removeFirst()
            guard let transfer = transfers[url] else { continue }
            active += 1
            transfer.task = Task {
                let data = await Self.download(transfer.url, session: session, redirects: redirects, cache: cache) { loaded, total in
                    guard self.transfers[url] === transfer else { return }
                    for callback in transfer.progress.values { callback(loaded, total) }
                }
                active -= 1
                if transfers[url] === transfer {
                    transfers.removeValue(forKey: url)
                    for callback in transfer.callbacks.values { callback(data) }
                }
                startWaiting()
            }
        }
    }

    private nonisolated static func download(
        _ url: URL, session: URLSession, redirects: AppleImageRedirects, cache: AppleImageDiskCache,
        progress: @MainActor (Int, Int) -> Void
    ) async -> Data? {
        do {
            let stored = await cache.read(url)
            let cached = stored.flatMap { entry in
                entry.chain.allSatisfy { redirects.allowedURL(URL(string: $0)) != nil } ? entry : nil
            }
            try Task.checkCancellation()
            if let cached, cached.expires > Date() {
                await progress(cached.body.count, cached.body.count)
                return cached.body
            }
            var request = URLRequest(url: url)
            if let cached {
                request.setValue(cached.etag, forHTTPHeaderField: "If-None-Match")
                request.setValue(cached.modified, forHTTPHeaderField: "If-Modified-Since")
            }
            let tracker = redirects.tracking()
            let (bytes, response) = try await session.bytes(for: request, delegate: tracker)
            guard let http = response as? HTTPURLResponse, let final = redirects.allowedURL(http.url),
                  http.expectedContentLength <= Int64(appleImageFileLimit) else {
                bytes.task.cancel()
                return nil
            }
            let chain = tracker.chain(from: url, to: final)
            guard chain.count <= 22, chain.allSatisfy({ redirects.allowedURL(URL(string: $0)) != nil }) else {
                bytes.task.cancel(); return nil
            }
            if http.statusCode == 304, let cached, cached.chain.last == final.absoluteString {
                bytes.task.cancel()
                // Preserve previously authorized hops as well as the revalidation path.
                var seen = Set<String>()
                let provenance = (Array(cached.chain.dropFirst().dropLast()) + Array(chain.dropFirst().dropLast())).filter { seen.insert($0).inserted }
                let merged = [url.absoluteString] + provenance + [final.absoluteString]
                if tracker.allowsCaching() {
                    _ = await cache.store(url, body: cached.body, chain: merged, response: http, previous: cached)
                } else { await cache.remove(url) }
                await progress(cached.body.count, cached.body.count)
                return cached.body
            }
            guard (200...299).contains(http.statusCode) else { bytes.task.cancel(); return nil }
            var data = Data()
            let encoding = http.value(forHTTPHeaderField: "Content-Encoding")?.lowercased()
            let total = encoding == nil || encoding == "identity" ? max(0, Int(http.expectedContentLength)) : 0
            var nextReport = Date()
            var reported = 0
            for try await byte in bytes {
                guard data.count < appleImageFileLimit else {
                    bytes.task.cancel()
                    return nil
                }
                data.append(byte)
                if data.count.isMultiple(of: 4096), Date() >= nextReport {
                    await progress(data.count, data.count <= total ? total : 0)
                    reported = data.count
                    nextReport = Date().addingTimeInterval(0.1)
                }
            }
            if data.count > reported { await progress(data.count, data.count <= total ? total : 0) }
            try Task.checkCancellation()
            if tracker.allowsCaching() {
                _ = await cache.store(url, body: data, chain: chain, response: http, previous: nil)
            } else { await cache.remove(url) }
            return data
        } catch { return nil }
    }
}

enum AppleImageFit: String {
    case contain, cover, fill, none
    case scaleDown = "scale_down"
}

struct AppleImageRequest: Equatable {
    let source: String
    let pixelWidth: Int
    let pixelHeight: Int
    let fit: AppleImageFit
    var allowsAnimation = true
}

func appleAnimationDelay(_ properties: [String: Any]) -> Double? {
    let timingKeys = [
        (kCGImagePropertyGIFDictionary, kCGImagePropertyGIFUnclampedDelayTime, kCGImagePropertyGIFDelayTime),
        (kCGImagePropertyPNGDictionary, kCGImagePropertyAPNGUnclampedDelayTime, kCGImagePropertyAPNGDelayTime),
        (kCGImagePropertyWebPDictionary, kCGImagePropertyWebPUnclampedDelayTime, kCGImagePropertyWebPDelayTime),
        (kCGImagePropertyHEICSDictionary, kCGImagePropertyHEICSUnclampedDelayTime, kCGImagePropertyHEICSDelayTime),
    ]
    for (dictionary, unclamped, clamped) in timingKeys {
        guard let timing = properties[dictionary as String] as? [String: Any],
              let seconds = (timing[unclamped as String] ?? timing[clamped as String]) as? Double else { continue }
        return seconds.isFinite && seconds >= 0.02 ? min(seconds, 60) : 0.1
    }
    return nil
}

func appleIconIndices(_ input: CGImageSource, target: Int) -> (display: Int, natural: Int)? {
    let count = CGImageSourceGetCount(input)
    guard (2...256).contains(count) else { return nil }
    var candidates: [(index: Int, side: Int, indexed: Bool)] = []
    for index in 0..<count {
        guard let properties = CGImageSourceCopyPropertiesAtIndex(input, index, nil) as? [String: Any],
              let width = properties[kCGImagePropertyPixelWidth as String] as? Int,
              let height = properties[kCGImagePropertyPixelHeight as String] as? Int,
              width > 0, height > 0, width <= 16_384, height <= 16_384,
              width * height <= 64 * 1024 * 1024 else { continue }
        candidates.append((index, max(width, height), properties[kCGImagePropertyIsIndexed as String] as? Bool == true))
    }
    candidates.sort { lhs, rhs in
        if lhs.side != rhs.side { return lhs.side < rhs.side }
        if lhs.indexed != rhs.indexed { return !lhs.indexed }
        return lhs.index < rhs.index
    }
    guard let largestSide = candidates.last?.side,
          let natural = candidates.first(where: { $0.side == largestSide }) else { return nil }
    return (candidates.first(where: { $0.side >= target })?.index ?? natural.index, natural.index)
}

let receiveAppleSvgRaster: JadixNativeSvgRaster = { context, pixels, width, height, naturalWidth, naturalHeight in
    guard let context, let pixels, let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else { return }
    let rowBytes = Int(width) * 4
    let data = Data(bytes: pixels, count: rowBytes * Int(height))
    guard let provider = CGDataProvider(data: data as CFData),
          let image = CGImage(width: Int(width), height: Int(height), bitsPerComponent: 8, bitsPerPixel: 32,
                              bytesPerRow: rowBytes, space: colorSpace,
                              bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                              provider: provider, decode: nil, shouldInterpolate: true,
                              intent: .defaultIntent) else { return }
    context.assumingMemoryBound(to: AppleDecodedImage?.self).pointee = AppleDecodedImage(
        frames: [image], frameDelays: [],
        originalSize: CGSize(width: CGFloat(naturalWidth).rounded(.up), height: CGFloat(naturalHeight).rounded(.up))
    )
}

func decodeAppleSVG(_ data: Data, request: AppleImageRequest) -> AppleDecodedImage? {
    let fit: UInt8
    switch request.fit {
    case .contain: fit = 0
    case .cover, .fill: fit = 1
    case .scaleDown: fit = 2
    case .none: fit = 3
    }
    var decoded: AppleDecodedImage?
    withUnsafeMutablePointer(to: &decoded) { result in
        data.withUnsafeBytes { bytes in
            jadix_native_rasterize_svg(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count,
                                       UInt32(request.pixelWidth), UInt32(request.pixelHeight), fit,
                                       result, receiveAppleSvgRaster)
        }
    }
    return decoded
}

func decodeAppleImage(_ request: AppleImageRequest, remoteData: Data? = nil) -> AppleDecodedImage? {
    let key = "\(request.fit.rawValue):\(request.pixelWidth)x\(request.pixelHeight):\(request.allowsAnimation):\(request.source)" as NSString
    let embedded = request.source.hasPrefix(appleInlineImagePrefix)
    let cacheable = embedded || request.source.hasPrefix(appleAssetPrefix)
    if cacheable, let cached = appleImageCache.object(forKey: key) { return cached }
    let inputOptions = [kCGImageSourceShouldCache as String: false]
    guard let data = remoteData ?? appleImageData(request.source) else { return nil }
    guard !request.source.hasPrefix("data:image/svg+xml;base64,"),
          let input = CGImageSourceCreateWithData(data as CFData, inputOptions as CFDictionary),
          CGImageSourceGetType(input).map({ ($0 as String) != "public.svg-image" }) == true,
          CGImageSourceGetCount(input) > 0 else {
        guard let decoded = decodeAppleSVG(data, request: request) else { return nil }
        if cacheable, let frame = decoded.frames.first {
            appleImageCache.setObject(decoded, forKey: key, cost: frame.bytesPerRow * frame.height + key.length * 2)
        }
        return decoded
    }
    let frameCount = CGImageSourceGetCount(input)
    let format = CGImageSourceGetType(input).map { $0 as String } ?? ""
    let isSequence = frameCount > 1 && ["com.compuserve.gif", "public.png", "org.webmproject.webp",
                                      "public.heics", "public.avis", "public.avif", "public.jpeg-xl"].contains(format)
    let icon = ["com.microsoft.ico", "com.apple.icns", "com.microsoft.cur"].contains(format)
        ? appleIconIndices(input, target: max(request.pixelWidth, request.pixelHeight)) : nil
    let firstIndex = icon?.display ?? (isSequence ? 0 : min(CGImageSourceGetPrimaryImageIndex(input), frameCount - 1))
    guard let properties = CGImageSourceCopyPropertiesAtIndex(input, firstIndex, nil) as? [String: Any],
          let width = properties[kCGImagePropertyPixelWidth as String] as? Int,
          let height = properties[kCGImagePropertyPixelHeight as String] as? Int,
          width > 0, height > 0, width <= 16_384, height <= 16_384,
          width * height <= 64 * 1024 * 1024 else { return nil }
    let orientation = properties[kCGImagePropertyOrientation as String] as? Int ?? 1
    let rotates = !embedded && (5...8).contains(orientation)
    let displayWidth = rotates ? height : width
    let displayHeight = rotates ? width : height
    var originalSize = CGSize(width: CGFloat(displayWidth), height: CGFloat(displayHeight))
    if let icon, icon.natural != firstIndex,
       let natural = CGImageSourceCopyPropertiesAtIndex(input, icon.natural, nil) as? [String: Any],
       let naturalWidth = natural[kCGImagePropertyPixelWidth as String] as? Int,
       let naturalHeight = natural[kCGImagePropertyPixelHeight as String] as? Int {
        originalSize = CGSize(width: CGFloat(naturalWidth), height: CGFloat(naturalHeight))
    }
    let animates = request.allowsAnimation && isSequence
    // First, current and prefetched frame together fit the decode budget.
    let pixelBudget = animates ? 1024 * 1024 : 4 * 1024 * 1024
    let scaleX = Double(request.pixelWidth) / Double(displayWidth)
    let scaleY = Double(request.pixelHeight) / Double(displayHeight)
    let fitScale: Double
    switch request.fit {
    case .contain, .scaleDown: fitScale = min(scaleX, scaleY)
    case .cover, .fill: fitScale = max(scaleX, scaleY)
    case .none: fitScale = 1
    }
    let scale = min(1, fitScale, 4096 / Double(max(width, height)),
                    sqrt(Double(pixelBudget) / Double(width * height)))
    var frames: [CGImage]
    let delays: [Double] = []
    var sequence: AppleImageSequence?
    var decodedBytes: Int
    if scale < 1 || !embedded || animates {
        let maxSide = max(1, Int(Double(max(width, height)) * scale))
        let options: [String: Any] = [
            kCGImageSourceShouldCacheImmediately as String: true,
            kCGImageSourceCreateThumbnailFromImageAlways as String: true,
            kCGImageSourceCreateThumbnailWithTransform as String: !embedded,
            kCGImageSourceThumbnailMaxPixelSize as String: maxSide,
        ]
        guard let first = CGImageSourceCreateThumbnailAtIndex(input, firstIndex, options as CFDictionary) else { return nil }
        frames = [first]
        decodedBytes = first.bytesPerRow * first.height
        if animates {
            sequence = AppleImageSequence(input: input, options: options as CFDictionary,
                                           count: frameCount, firstDelay: appleAnimationDelay(properties) ?? 0.1)
            decodedBytes += data.count
        }
    } else {
        let options = [kCGImageSourceShouldCacheImmediately as String: true]
        guard let first = CGImageSourceCreateImageAtIndex(input, firstIndex, options as CFDictionary) else { return nil }
        frames = [first]
        decodedBytes = first.bytesPerRow * first.height
    }
    let decoded = AppleDecodedImage(
        frames: frames,
        frameDelays: delays,
        originalSize: originalSize,
        sequence: sequence
    )
    if cacheable {
        appleImageCache.setObject(decoded, forKey: key, cost: decodedBytes + key.length * 2)
    }
    return decoded
}


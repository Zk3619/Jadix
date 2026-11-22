import UIKit
import QuartzCore

@MainActor
final class JadixImagePlayback {
    private weak var view: UIView?
    private weak var layer: CALayer?
    private let sequence: AppleImageSequence
    private let first: CGImage
    private let blur: CGFloat
    private var timer: Timer?
    private var deadline: CFTimeInterval
    private var nextIndex = 1
    private var ready: (CGImage, Double)?
    private var running = true
    private var paused = false

    init(view: UIView, layer: CALayer, sequence: AppleImageSequence, first: CGImage, blur: CGFloat) {
        self.view = view; self.layer = layer; self.sequence = sequence; self.blur = blur; self.first = JadixRasterEffects.blur(first, radius: blur)
        deadline = CACurrentMediaTime() + sequence.firstDelay
        prefetch()
        let timer = Timer(timeInterval: 1.0 / 120, repeats: true) { [weak self] _ in self?.tick() }
        self.timer = timer
        RunLoop.main.add(timer, forMode: .common)
    }
    deinit { timer?.invalidate() }
    func stop() {
        running = false; timer?.invalidate(); timer = nil; ready = nil
        layer?.contents = first
    }
    private func prefetch() {
        let index = nextIndex, sequence = sequence
        appleImageQueue.async { [weak self] in
            let frame = sequence.frame(index)
            DispatchQueue.main.async { [weak self] in
                guard let self, self.running else { return }
                guard let frame else { self.stop(); return }
                self.ready = (JadixRasterEffects.blur(frame.0, radius: self.blur), frame.1)
            }
        }
    }
    private func tick() {
        guard running, let view, let layer, view.window != nil else { stop(); return }
        let now = CACurrentMediaTime()
        if UIAccessibility.isReduceMotionEnabled {
            layer.contents = first; paused = true; return
        }
        if !jadixSurfaceVisible(view) {
            paused = true; return
        }
        if paused { deadline = now + sequence.firstDelay; paused = false }
        guard now >= deadline, let (frame, delay) = ready else { return }
        ready = nil
        CATransaction.begin(); CATransaction.setDisableActions(true)
        layer.contents = frame
        CATransaction.commit()
        deadline = max(deadline + delay, now)
        nextIndex = (nextIndex + 1) % sequence.count
        prefetch()
    }
}

final class JadixImageView: UIView {
    private var source: String?
    private var placeholder: String?
    private var placeholderRequest: UUID?
    private var placeholderLoad: (url: URL, token: UUID)?
    private var showingMainImage = false
    private var fadeSeconds = 0.15
    private var blurRadius: CGFloat = 0
    private var fit: AppleImageFit = .contain
    private var fraction = CGPoint(x: 0.5, y: 0.5)
    private var offset = CGPoint.zero
    private var sourceRevision: UInt64 = 0
    private var loadStarted = false
    private var loadEnded = false
    private var reportedBytes = 0
    private var lastRequest: AppleImageRequest?
    private var decodeID: UUID?
    private var remoteLoad: (url: URL, token: UUID)?
    private let remoteImages: AppleRemoteImages
    private var originalSize: CGSize?
    private var displayedImage: AppleDecodedImage?
    private var playbackLayer: CALayer?
    private var playback: JadixImagePlayback?
    var onOriginalSizeChange: (() -> Void)?
    var onLoadEvent: (([String: Any]) -> Void)?

    init(remoteImages: AppleRemoteImages) {
        self.remoteImages = remoteImages
        super.init(frame: .zero)
    }

    required init?(coder: NSCoder) { nil }
    func dispose() { onLoadEvent = nil; onOriginalSizeChange = nil; setSource(nil) }
    override func sizeThatFits(_ size: CGSize) -> CGSize {
        guard let originalSize else { return .zero }; return CGSize(width: min(size.width, originalSize.width), height: min(size.height, originalSize.height))
    }

    func originalSize(for requestedSource: String) -> CGSize? {
        source == requestedSource ? originalSize : nil
    }

    func setSource(_ next: String?, style: [String: Any]? = nil) {
        let nextPlaceholder = style?["image_placeholder"] as? String
        if source != next {
            cancelDecode()
            cancelPlaceholder()
            source = next
            sourceRevision &+= 1
            loadStarted = false
            loadEnded = false
            reportedBytes = 0
            lastRequest = nil
            originalSize = nil
            clearImage()
        } else if placeholder != nextPlaceholder {
            cancelPlaceholder()
            if !showingMainImage { clearImage() }
        }
        placeholder = nextPlaceholder
        fadeSeconds = (style?["image_fade_ms"] as? Double).map { $0 / 1000 } ?? 0.15
        let blur = CGFloat(style?["image_blur_radius"] as? Double ?? 0)
        if blur != blurRadius { playback?.stop(); playback = nil; blurRadius = blur; if let first = displayedImage?.frames.first { playbackLayer?.contents = JadixRasterEffects.blur(first, radius: blur) } }
        fit = (style?["image_fit"] as? String).flatMap(AppleImageFit.init(rawValue:)) ?? .contain
        let position = style?["image_position"] as? [String: Any]
        fraction = imagePositionPoint(position?["fraction"]) ?? CGPoint(x: 0.5, y: 0.5)
        offset = imagePositionPoint(position?["offset"]) ?? .zero
        layoutImage()
        updatePlayback()
        decodeLatest()
        decodePlaceholder()
    }

    private func clearImage() {
        playback?.stop(); playback = nil
        showingMainImage = false
        displayedImage = nil
        playbackLayer?.removeFromSuperlayer()
        playbackLayer = nil
        layer.removeAnimation(forKey: "image-load")
    }

    private func imagePositionPoint(_ value: Any?) -> CGPoint? {
        guard let pair = value as? [Double], pair.count == 2,
              pair[0].isFinite, pair[1].isFinite else { return nil }
        return CGPoint(x: pair[0], y: pair[1])
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil {
            if decodeID != nil { lastRequest = nil }
            cancelDecode()
            if displayedImage == nil { cancelPlaceholder() }
        }
        updatePlayback()
        decodeLatest()
        decodePlaceholder()
    }

    override func traitCollectionDidChange(_ previousTraitCollection: UITraitCollection?) {
        super.traitCollectionDidChange(previousTraitCollection)
        refreshBackingDensity()
    }

    func refreshBackingDensity() {
        decodeLatest()
        decodePlaceholder()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        layoutImage()
    }

    private func layoutImage() {
        guard let playbackLayer, let backingLayer = playbackLayer.superlayer,
              let picture = displayedImage else { return }
        let bounds = backingLayer.bounds
        let natural = picture.originalSize
        let scaleX = bounds.width / natural.width
        let scaleY = bounds.height / natural.height
        let scale: CGFloat
        switch fit {
        case .contain: scale = min(scaleX, scaleY)
        case .cover: scale = max(scaleX, scaleY)
        case .fill, .none: scale = 1
        case .scaleDown: scale = min(1, scaleX, scaleY)
        }
        let size = fit == .fill ? bounds.size : CGSize(width: natural.width * scale, height: natural.height * scale)
        let x = (bounds.width - size.width) * fraction.x + offset.x
        let y = (bounds.height - size.height) * fraction.y + offset.y
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        playbackLayer.frame = CGRect(x: bounds.minX + x,
                                     y: bounds.minY + y,
                                     width: size.width, height: size.height)
        CATransaction.commit()
    }

    private func updatePlayback() {
        guard showingMainImage, let layer = playbackLayer, let picture = displayedImage,
              let sequence = picture.sequence, let first = picture.frames.first,
              window != nil else {
            playback?.stop(); playback = nil
            return
        }
        if playback == nil { playback = JadixImagePlayback(view: self, layer: layer, sequence: sequence, first: first, blur: blurRadius) }
    }

    private func display(_ picture: AppleDecodedImage, isMain: Bool) {
        guard let first = picture.frames.first else { return }; let backingLayer = layer
        if isMain, !showingMainImage, fadeSeconds > 0,
           source?.hasPrefix(appleInlineImagePrefix) == false,
           !UIAccessibility.isReduceMotionEnabled {
            let fade = CATransition()
            fade.type = .fade
            fade.duration = fadeSeconds
            backingLayer.add(fade, forKey: "image-load")
        }
        playback?.stop(); playback = nil
        showingMainImage = isMain
        displayedImage = picture
        playbackLayer?.removeFromSuperlayer()
        let playback = CALayer()
        backingLayer.masksToBounds = true
        playback.contents = JadixRasterEffects.blur(first, radius: blurRadius)
        backingLayer.addSublayer(playback)
        playbackLayer = playback
        layoutImage()
        updatePlayback()
    }

    private var imageRequest: AppleImageRequest? {
        guard let source, window != nil else { return nil }
        let density = traitCollection.displayScale
        let pixels = CGSize(width: bounds.width * density, height: bounds.height * density)
        guard pixels.width.isFinite, pixels.height.isFinite else { return nil }
        return AppleImageRequest(
            source: source,
            pixelWidth: Int((min(max(pixels.width, 64), 4096) / 64).rounded(.up)) * 64,
            pixelHeight: Int((min(max(pixels.height, 64), 4096) / 64).rounded(.up)) * 64,
            fit: fit
        )
    }

    private func cancelDecode() {
        decodeID = nil
        if let remoteLoad {
            remoteImages.cancel(remoteLoad.url, token: remoteLoad.token)
            self.remoteLoad = nil
        }
    }

    private func cancelPlaceholder() {
        if let load = placeholderLoad { remoteImages.cancel(load.url, token: load.token) }
        placeholderLoad = nil
        placeholderRequest = nil
    }

    private func decodePlaceholder() {
        guard !showingMainImage, placeholderRequest == nil, let placeholder,
              let main = imageRequest, placeholder != main.source else { return }
        let revision = UUID()
        placeholderRequest = revision
        let request = AppleImageRequest(source: placeholder, pixelWidth: main.pixelWidth,
                                         pixelHeight: main.pixelHeight, fit: .cover)
        placeholderLoad = remoteImages.decode(request, completion: { [weak self] decoded in
            guard let self, self.placeholderRequest == revision else { return }
            self.placeholderLoad = nil
            if let decoded, !self.showingMainImage { self.display(decoded, isMain: false) }
        })
    }

    private func decodeLatest() {
        guard decodeID == nil, let request = imageRequest, request != lastRequest else { return }
        let id = UUID()
        lastRequest = request
        decodeID = id
        let revision = sourceRevision
        if !loadStarted {
            loadStarted = true
            reportLoadEvents([["kind": "imageLoadStart"]], revision: revision)
        }
        remoteLoad = remoteImages.decode(request) { [weak self] decoded in
            guard let self, self.decodeID == id else { return }
            self.remoteLoad = nil
            self.decodeID = nil
            self.finishDecode(request, revision: revision, decoded: decoded)
        } networkFinished: { [weak self] in
            guard let self, self.decodeID == id else { return }
            self.remoteLoad = nil
        } progress: { [weak self] loaded, total in
            guard let self, self.decodeID == id, !self.loadEnded, loaded > self.reportedBytes else { return }
            self.reportedBytes = loaded
            self.reportLoadEvents([["kind": "imageProgress", "loaded": loaded, "total": total]], revision: revision)
        }
    }

    private func finishDecode(_ request: AppleImageRequest, revision: UInt64, decoded: AppleDecodedImage?) {
        if sourceRevision == revision, let current = imageRequest {
            if let decoded {
                if originalSize != decoded.originalSize {
                    originalSize = decoded.originalSize
                    onOriginalSizeChange?()
                }
                if current == request {
                    cancelPlaceholder()
                    display(decoded, isMain: true)
                }
            }
            if current == request, !loadEnded {
                loadEnded = true
                let result: [String: Any]
                if let decoded {
                    result = ["kind": "imageLoad", "width": Int(decoded.originalSize.width),
                              "height": Int(decoded.originalSize.height)]
                } else {
                    result = ["kind": "imageError", "message": "Unable to load or decode the image."]
                }
                reportLoadEvents([result, ["kind": "imageLoadEnd"]], revision: revision)
            }
        }
        decodeLatest()
    }

    private func reportLoadEvents(_ events: [[String: Any]], revision: UInt64) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            for event in events {
                guard self.sourceRevision == revision else { return }
                self.onLoadEvent?(event)
            }
        }
    }
}


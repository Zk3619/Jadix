import UIKit

/// UIKit coordinates are scaled pixels; the shared contract uses document points.
@MainActor final class JadixScrollView: UIScrollView {
    let document = UIView()
    var observers: [UUID: (CGPoint, CGSize, CGFloat) -> Void] = [:]
    func stop() {
        setContentOffset(contentOffset, animated: false)
        let enabled = panGestureRecognizer.isEnabled
        panGestureRecognizer.isEnabled = false; panGestureRecognizer.isEnabled = enabled
        layer.removeAllAnimations()
    }
    func retire() {
        delegate = nil; observers.removeAll(); reportPending = false; stop()
    }
    private var configuration: NSDictionary?
    private var lastScrollTarget: [Double]?
    // Complete layout extent, independent of currently materialized subviews.
    var layoutExtent: CGSize?
    private var cullingOffset: CGPoint?
    private var cullingSample = ProcessInfo.processInfo.systemUptime
    private var cullingVelocity = CGPoint.zero
    func cullingViewport() -> [String: Any] {
        let now = ProcessInfo.processInfo.systemUptime, offset = logicalOffset
        if let previous = cullingOffset, previous != offset {
            let elapsed = max(1.0 / 240, now - cullingSample)
            cullingVelocity = CGPoint(x: (offset.x - previous.x) / elapsed, y: (offset.y - previous.y) / elapsed)
            cullingSample = now
        }
        cullingOffset = offset
        return ["offset": [offset.x, offset.y], "size": [viewport.width, viewport.height],
                "velocity": [cullingVelocity.x, cullingVelocity.y], "elapsed": now - cullingSample]
    }
    var reportPending = false
    var reportedOffset = CGPoint(x: -1, y: -1), reportedViewport = CGSize.zero
    var reportedScale: CGFloat = 0
    override init(frame: CGRect) {
        super.init(frame: frame); addSubview(document)
        contentInsetAdjustmentBehavior = .never; automaticallyAdjustsScrollIndicatorInsets = false; bouncesZoom = false
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    var logicalOffset: CGPoint { CGPoint(x: max(0, contentOffset.x / zoomScale), y: max(0, contentOffset.y / zoomScale)) }
    var viewport: CGSize { CGSize(width: bounds.width / zoomScale, height: bounds.height / zoomScale) }
    func configure(_ value: [String: Any]?) {
        let next = value.map { NSDictionary(dictionary: $0) }
        guard configuration?.isEqual(next) != true || (configuration == nil && next != nil) else { return }
        let first = configuration == nil; configuration = next
        minimumZoomScale = CGFloat(value?["min"] as? Double ?? 1)
        maximumZoomScale = CGFloat(value?["max"] as? Double ?? 1)
        pinchGestureRecognizer?.isEnabled = value?["gestures"] as? Bool ?? false
        setZoomScale(first ? CGFloat(value?["initial"] as? Double ?? 1) : min(maximumZoomScale, max(minimumZoomScale, zoomScale)), animated: false)
    }
    func applyStyle(_ style: [String: Any]?) {
        configure(style?["scroll_zoom"] as? [String: Any])
        let bars = style?["scrollbars"] as? [Bool] ?? [true, true]
        showsHorizontalScrollIndicator = bars[0]; showsVerticalScrollIndicator = bars[1]
        bounces = style?["scroll_bounces"] as? Bool ?? true
        let inset = style?["scrollbar_inset"] as? [Double] ?? [0, 0, 0, 0]
        scrollIndicatorInsets = UIEdgeInsets(top: inset[0], left: inset[3], bottom: inset[2], right: inset[1])
        updateExtent()
        guard let target = style?["scroll_to"] as? [Double], target.count == 2 else { lastScrollTarget = nil; return }
        guard target != lastScrollTarget else { return }; lastScrollTarget = target
        let point = CGPoint(x: min(max(0, target[0] * zoomScale), max(0, contentSize.width - bounds.width)),
                            y: min(max(0, target[1] * zoomScale), max(0, contentSize.height - bounds.height)))
        setContentOffset(point, animated: style?["scroll_behavior"] as? String == "smooth")
    }
    func updateExtent() {
        let size = layoutExtent ?? CGSize(width: max(0, document.subviews.map { $0.frame.maxX }.max() ?? 0), height: max(0, document.subviews.map { $0.frame.maxY }.max() ?? 0))
        document.bounds = CGRect(origin: .zero, size: size)
        document.center = CGPoint(x: size.width * zoomScale / 2, y: size.height * zoomScale / 2)
        contentSize = CGSize(width: size.width * zoomScale, height: size.height * zoomScale)
        scroll(to: logicalOffset)
    }
    func scroll(to value: CGPoint) {
        let point = CGPoint(x: min(max(0, value.x * zoomScale), max(0, contentSize.width - bounds.width)),
                            y: min(max(0, value.y * zoomScale), max(0, contentSize.height - bounds.height)))
        if point != contentOffset { setContentOffset(point, animated: false) }
    }
    func zoom(to scale: CGFloat, anchor: CGPoint) {
        let point = CGPoint(x: min(bounds.width, max(0, anchor.x)), y: min(bounds.height, max(0, anchor.y)))
        let old = logicalOffset, previous = zoomScale
        setZoomScale(min(maximumZoomScale, max(minimumZoomScale, scale)), animated: false)
        scroll(to: CGPoint(x: old.x + point.x / previous - point.x / zoomScale, y: old.y + point.y / previous - point.y / zoomScale))
    }
}

/// A borrowed native viewport. Removal, root replacement or detach invalidates it.
@MainActor public final class JadixScrollHandle {
    private weak var scroll: JadixScrollView?
    private var valid: (() -> Bool)?
    private var listeners: [UUID: (CGPoint, CGSize, CGFloat) -> Void] = [:]
    private let subscription = UUID()
    init(_ scroll: JadixScrollView, valid: @escaping () -> Bool) {
        self.scroll = scroll; self.valid = valid
        scroll.observers[subscription] = { [weak self] offset, viewport, scale in
            guard let self, self.isAttached else { return }
            for callback in Array(self.listeners.values) { callback(offset, viewport, scale) }
        }
    }
    public var isAttached: Bool { scroll != nil && valid?() == true }
    public var nativeView: UIScrollView? { isAttached ? scroll : nil }
    public var offset: CGPoint? { isAttached ? scroll?.logicalOffset : nil }
    public var viewport: CGSize? { isAttached ? scroll?.viewport : nil }
    public var scale: CGFloat? { isAttached ? scroll?.zoomScale : nil }
    @discardableResult public func scroll(to offset: CGPoint) -> Bool {
        guard isAttached, offset.x.isFinite, offset.y.isFinite, offset.x >= 0, offset.y >= 0 else { return false }
        scroll?.scroll(to: offset); return true
    }
    @discardableResult public func zoom(to scale: CGFloat, anchor: CGPoint) -> Bool {
        guard isAttached, scale.isFinite, scale > 0, anchor.x.isFinite, anchor.y.isFinite else { return false }
        scroll?.zoom(to: scale, anchor: anchor); return true
    }
    @discardableResult public func stop() -> Bool {
        guard isAttached else { return false }; scroll?.stop(); return true
    }
    @discardableResult public func observe(_ callback: @escaping (CGPoint, CGSize, CGFloat) -> Void) -> UUID? {
        guard isAttached, listeners.count < 64 else { return nil }
        let id = UUID(); listeners[id] = callback; return id
    }
    public func removeObserver(_ id: UUID) { listeners.removeValue(forKey: id) }
    public func detach() {
        scroll?.observers.removeValue(forKey: subscription)
        scroll = nil; valid = nil; listeners.removeAll()
    }
    deinit {
        // UIKit access stays on its actor, including last release from foreign code.
        let scroll = scroll, subscription = subscription
        Task { @MainActor [weak scroll] in scroll?.observers.removeValue(forKey: subscription) }
    }
}

// Platform-independent Apple resource policy, navigation and playback lifetimes.
import Foundation
import WebKit
import AVKit
import AVFoundation
import UniformTypeIdentifiers
#if os(iOS)
import UIKit
func jadixSurfaceVisible(_ view: UIView) -> Bool {
    guard view.window != nil, UIApplication.shared.applicationState == .active else { return false }
    var current: UIView? = view
    while let value = current { if value.isHidden || value.alpha == 0 { return false }; current = value.superview }
    return !view.convert(view.bounds, to: view.window).intersection(view.window!.bounds).isEmpty
}
#else
import AppKit
func jadixSurfaceVisible(_ view: NSView) -> Bool {
    view.window?.isVisible == true && !view.isHiddenOrHasHiddenAncestor && !view.visibleRect.isEmpty
}
#endif
struct JadixWebResourceResponse { let status: Int; let mime: String; let bytes: Data }
typealias JadixWebResourceProvider = (URLRequest, @escaping (Result<JadixWebResourceResponse, Error>) -> Void) -> (() -> Void)?
// app://bundle/path is a read-only bundle capability. Canonical containment is
// checked after percent decoding and resolving symlinks; file:// gets no WebKit
// directory-read grant. The same resolver is used by the media surface.
func jadixBundleResource(_ url: URL) -> URL? {
    guard let root = Bundle.main.resourceURL?.resolvingSymlinksInPath(), url.query == nil, url.fragment == nil,
          url.user == nil, url.password == nil else { return nil }
    let path: String
    if url.scheme == "app", url.host == "bundle" { path = String(url.path.dropFirst()) }
    else if url.scheme == "asset" { path = (url.host.map { $0 + "/" } ?? "") + String(url.path.dropFirst()) }
    else { return nil }
    guard !path.isEmpty, !path.contains("\\"), !path.utf8.contains(0),
          path.split(separator: "/").allSatisfy({ $0 != "." && $0 != ".." }) else { return nil }
    let file = root.appendingPathComponent(path).resolvingSymlinksInPath()
    guard file.path.hasPrefix(root.path + "/"),
          (try? file.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true else { return nil }
    return file
}
@MainActor final class JadixBundleScheme: NSObject, WKURLSchemeHandler {
    private struct Pending { let token: UUID; var cancel: (() -> Void)?; let timeout: DispatchWorkItem }
    private var requests: [ObjectIdentifier: Pending] = [:]
    var provider: JadixWebResourceProvider?
    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        let id = ObjectIdentifier(task)
        guard requests.count < 64, let url = task.request.url,
              ["GET", "HEAD"].contains(task.request.httpMethod ?? "GET") else {
            task.didFailWithError(URLError(.noPermissionsToReadFile)); return
        }
        let token = UUID()
        let timeout = DispatchWorkItem { [weak self] in
            guard let self, self.requests[id]?.token == token, let pending = self.requests.removeValue(forKey: id) else { return }
            pending.cancel?(); task.didFailWithError(URLError(.timedOut))
        }
        requests[id] = Pending(token: token, cancel: nil, timeout: timeout)
        DispatchQueue.main.asyncAfter(deadline: .now() + 30, execute: timeout)
        let finish: (Result<JadixWebResourceResponse, Error>) -> Void = { [weak self] result in
            DispatchQueue.main.async { [weak self] in
                guard let self, self.requests[id]?.token == token, let pending = self.requests.removeValue(forKey: id) else { return }
                pending.timeout.cancel()
                switch result {
                case .success(let response):
                    guard response.bytes.count <= 32 * 1024 * 1024, (200...599).contains(response.status),
                          response.mime.utf8.count <= 256, response.mime.contains("/"),
                          !response.mime.contains("\r"), !response.mime.contains("\n"),
                          let headers = HTTPURLResponse(url: url, statusCode: response.status, httpVersion: "HTTP/1.1",
                            headerFields: ["Content-Type": response.mime, "Content-Length": String(response.bytes.count)]) else {
                        task.didFailWithError(URLError(.badServerResponse)); return
                    }
                    task.didReceive(headers)
                    if task.request.httpMethod != "HEAD" { task.didReceive(response.bytes) }
                    task.didFinish()
                case .failure(let error): task.didFailWithError(error)
                }
            }
        }
        guard let file = jadixBundleResource(url) else {
            if let cancel = provider?(task.request, finish) { requests[id]?.cancel = cancel }
            else { finish(.failure(URLError(.noPermissionsToReadFile))) }
            return
        }
        let work = DispatchWorkItem {
            let result: Result<JadixWebResourceResponse, Error> = Result {
                let handle = try FileHandle(forReadingFrom: file); defer { try? handle.close() }
                let data = try handle.read(upToCount: 32 * 1024 * 1024 + 1) ?? Data()
                guard data.count <= 32 * 1024 * 1024 else { throw URLError(.dataLengthExceedsMaximum) }
                return JadixWebResourceResponse(status: 200, mime: UTType(filenameExtension: file.pathExtension)?.preferredMIMEType ?? "application/octet-stream", bytes: data)
            }
            finish(result)
        }
        requests[id]?.cancel = { work.cancel() }
        DispatchQueue.global(qos: .userInitiated).async(execute: work)
    }
    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {
        guard let pending = requests.removeValue(forKey: ObjectIdentifier(task)) else { return }
        pending.timeout.cancel(); pending.cancel?()
    }
    func cancelAll() {
        let pending = Array(requests.values); requests.removeAll()
        for request in pending { request.timeout.cancel(); request.cancel?() }
    }
}
@MainActor final class JadixHostedWeb: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
    let view: WKWebView
    let resources: JadixBundleScheme
    var resourceAllowed: ((URL) -> Bool)?
    var emit: ((String, Any) -> Void)?
    var documentChanged: ((Bool) -> Void)?
    private var source: NSDictionary?
    private var origins = Set<String>()
    private var generation: UInt64 = 0
    private var html = false
    override init() {
        let resources = JadixBundleScheme()
        self.resources = resources
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        configuration.setURLSchemeHandler(resources, forURLScheme: "app")
        configuration.setURLSchemeHandler(resources, forURLScheme: "asset")
        view = WKWebView(frame: .zero, configuration: configuration)
        super.init(); view.navigationDelegate = self
        configuration.userContentController.add(self, name: "jadix")
    }
    private func origin(_ url: URL) -> String? {
        guard url.scheme == "https", let host = url.host?.lowercased(), url.user == nil, url.password == nil else { return nil }
        return "https://" + host + (url.port.flatMap { $0 == 443 ? nil : ":\($0)" } ?? "")
    }
    private func allowed(_ url: URL) -> Bool {
        if url.scheme == "app" || url.scheme == "asset" { return jadixBundleResource(url) != nil || resourceAllowed?(url) == true }
        if html && url.absoluteString == "about:blank" { return true }
        return origin(url).map { origins.contains($0) } ?? false
    }
    func update(_ value: [String: Any]) {
        let next = value as NSDictionary
        if source == next { return }
        source = next; generation &+= 1; let current = generation
        documentChanged?(false); resources.cancelAll(); view.stopLoading(); html = value["html"] is String
        origins = Set((value["origins"] as? [String] ?? []).compactMap { URL(string: $0).flatMap(origin) })
        if let url = (value["url"] as? String).flatMap(URL.init(string:)), let origin = origin(url) { origins.insert(origin) }
        if let base = (value["base_url"] as? String).flatMap(URL.init(string:)), let origin = origin(base) { origins.insert(origin) }
        // Restrict network subresources as well as top-level/frame navigations.
        var rules: [[String: Any]] = [["trigger": ["url-filter": "^https?://"], "action": ["type": "block"]]]
        for value in origins.sorted() {
            rules.append(["trigger": ["url-filter": "^" + NSRegularExpression.escapedPattern(for: value) + "/"],
                          "action": ["type": "ignore-previous-rules"]])
        }
        guard let data = try? JSONSerialization.data(withJSONObject: rules), let json = String(data: data, encoding: .utf8) else { return }
        WKContentRuleListStore.default().compileContentRuleList(forIdentifier: UUID().uuidString, encodedContentRuleList: json) { [weak self] list, error in
            if let list { WKContentRuleListStore.default().removeContentRuleList(forIdentifier: list.identifier, completionHandler: nil) }
            guard let self, self.generation == current else { return }
            guard let list, error == nil else { self.emit?("error", "Unable to apply web resource policy"); return }
            self.view.configuration.userContentController.removeAllContentRuleLists()
            self.view.configuration.userContentController.add(list)
            if let content = value["html"] as? String {
                let base = (value["base_url"] as? String).flatMap(URL.init(string:))
                guard base == nil || self.allowed(base!) else { self.emit?("error", "Web base URL is outside its resource policy"); return }
                self.view.loadHTMLString(content, baseURL: base)
            } else if let url = (value["url"] as? String).flatMap(URL.init(string:)), self.allowed(url) {
                self.view.load(URLRequest(url: url))
            } else { self.emit?("error", "Web URL is outside its resource policy") }
        }
    }
    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = navigationAction.request.url, allowed(url), navigationAction.targetFrame != nil else {
            if let url = navigationAction.request.url { emit?("navigation", String(url.absoluteString.prefix(4096))) }
            decisionHandler(.cancel); return
        }
        if navigationAction.targetFrame?.isMainFrame == true { resources.cancelAll() }
        decisionHandler(.allow)
    }
    func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) { documentChanged?(false); emit?("loading", true) }
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) { documentChanged?(true); emit?("loading", false) }
    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) { documentChanged?(false); emit?("loading", false); emit?("error", String(error.localizedDescription.prefix(4096))) }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) { self.webView(webView, didFail: navigation, withError: error) }
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) { documentChanged?(false); emit?("error", "Web content process terminated") }
    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame, let url = message.frameInfo.request.url, allowed(url),
              JSONSerialization.isValidJSONObject([message.body]),
              let data = try? JSONSerialization.data(withJSONObject: [message.body]), data.count <= 16 * 1024 else { return }
        emit?("message", message.body)
    }
    func dispose() {
        generation &+= 1; documentChanged?(false); documentChanged = nil; emit = nil
        resources.cancelAll(); view.stopLoading(); view.navigationDelegate = nil
        view.configuration.userContentController.removeScriptMessageHandler(forName: "jadix")
        view.configuration.userContentController.removeAllContentRuleLists()
        view.removeFromSuperview()
    }
}

@MainActor final class JadixHostedMedia: NSObject {
    var requiresWindow = true
    #if os(iOS)
    let controller = AVPlayerViewController()
    var view: UIView { controller.view }
#else
    let view = AVPlayerView()
#endif
    var emit: ((String, Any) -> Void)?
    private var player: AVPlayer?
    private var source: String?
    private var seekRevision: UInt64?
    private var generation = UUID()
    private var seekGeneration = UUID()
    private var clock: Any?
    private var notifications: [NSObjectProtocol] = []
    private var observations: [NSKeyValueObservation] = []
    private var visibility: Timer?
    private var progressInterval = 0.25
    private var paused = true
    private var looping = false
    private var audio = false
    private var rate: Float = 1
    private var reachedEnd = false
    private var seeking = false
    private var failed = false
    private var lastBuffering: Bool?
    private var lastDuration: Double?
    private let sessionToken = UUID()

    func update(_ value: [String: Any]) {
        let wasPaused = paused
        paused = value["paused"] as? Bool ?? true; looping = value["looping"] as? Bool ?? false
        if wasPaused && !paused { JadixAudioSession.shared.requestPlayback() }
        audio = value["audio"] as? Bool ?? false; rate = (value["rate"] as? NSNumber)?.floatValue ?? 1
        #if os(iOS)
        controller.showsPlaybackControls = value["controls"] as? Bool == true
#else
        view.controlsStyle = value["controls"] as? Bool == true ? .floating : .none
#endif
        #if os(iOS)
        controller.videoGravity = value["fit"] as? String == "cover" ? .resizeAspectFill : value["fit"] as? String == "fill" ? .resize : .resizeAspect
#else
        view.videoGravity = value["fit"] as? String == "cover" ? .resizeAspectFill : value["fit"] as? String == "fill" ? .resize : .resizeAspect
#endif
        let interval = min(60, max(0.016, ((value["progress_interval_ms"] as? NSNumber)?.doubleValue ?? 250) / 1000))
        let changedInterval = progressInterval != interval
        progressInterval = interval
        if source != value["source"] as? String {
            dispose(); source = value["source"] as? String; seekRevision = nil
            emit?("loadStart", true)
            guard let raw = source, let requested = URL(string: raw),
                  let url = jadixBundleResource(requested) ?? ((requested.scheme == "https" && requested.host?.isEmpty == false && requested.user == nil && requested.password == nil) || requested.isFileURL ? requested : nil) else {
                reportFailure("Unsupported media URL"); return
            }
            let item = AVPlayerItem(url: url), next = AVPlayer(playerItem: nil), token = generation
            player = next
#if os(iOS)
            controller.player = next
#else
            view.player = next
#endif
            observations.append(item.observe(\.status, options: [.initial, .new]) { [weak self] item, _ in
                DispatchQueue.main.async {
                    guard let self, self.generation == token else { return }
                    if item.status == .failed { self.reportFailure(item.error?.localizedDescription ?? "Media failed") }
                    else if item.status == .readyToPlay {
                        self.emit?("load", self.timeline()); self.reportTimeline(); self.syncPlayback()
                    }
                }
            })
            observations.append(next.observe(\.timeControlStatus, options: [.new]) { [weak self] player, _ in
                DispatchQueue.main.async {
                    guard let self, self.generation == token else { return }
                    self.emit?("playing", player.timeControlStatus == .playing); self.reportBuffering()
                }
            })
            observations.append(item.observe(\.duration, options: [.new]) { [weak self] _, _ in
                DispatchQueue.main.async { guard let self, self.generation == token else { return }; self.reportTimeline() }
            })
            observations.append(item.observe(\.loadedTimeRanges, options: [.new]) { [weak self] _, _ in
                DispatchQueue.main.async { guard let self, self.generation == token else { return }; self.reportTimeline(); self.reportBuffering() }
            })
            observations.append(item.observe(\.isPlaybackBufferEmpty, options: [.new]) { [weak self] _, _ in
                DispatchQueue.main.async { guard let self, self.generation == token else { return }; self.reportBuffering() }
            })
            next.replaceCurrentItem(with: item)
            installClock()
            notifications.append(NotificationCenter.default.addObserver(forName: .AVPlayerItemDidPlayToEndTime, object: item, queue: .main) { [weak self] _ in
                guard let self, self.generation == token else { return }
                self.reachedEnd = true; self.syncPlayback(); self.emit?("ended", true); self.reportTimeline()
                if self.looping { self.seek(to: 0) }
            })
            notifications.append(NotificationCenter.default.addObserver(forName: .AVPlayerItemFailedToPlayToEndTime, object: item, queue: .main) { [weak self] notification in
                guard let self, self.generation == token else { return }
                let error = notification.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
                self.reportFailure(error?.localizedDescription ?? "Media playback failed")
            })
            notifications.append(NotificationCenter.default.addObserver(forName: .jadixAudioSessionChanged, object: nil, queue: .main) { [weak self] _ in
                DispatchQueue.main.async { guard let self, self.generation == token else { return }; self.syncPlayback() }
            })
            visibility = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in self?.syncPlayback() }
        } else if changedInterval { installClock() }
        player?.volume = (value["volume"] as? NSNumber)?.floatValue ?? 1
        player?.isMuted = value["muted"] as? Bool ?? false
        let revision = (value["seek_revision"] as? NSNumber)?.uint64Value
        if revision != seekRevision {
            seekRevision = revision
            if let seconds = value["seek"] as? Double { seek(to: seconds) }
        } else if reachedEnd && wasPaused && !paused { seek(to: 0) }
        syncPlayback()
    }
    private func installClock() {
        guard let player else { return }
        if let clock { player.removeTimeObserver(clock) }
        let token = generation
        clock = player.addPeriodicTimeObserver(forInterval: CMTime(seconds: progressInterval, preferredTimescale: 60_000), queue: .main) { [weak self] _ in
            guard let self, self.generation == token else { return }; self.reportTimeline()
        }
    }
    private func timeline() -> [String: Double] {
        guard let player, let item = player.currentItem else { return [:] }
        var result: [String: Double] = [:]
        let position = player.currentTime().seconds, duration = item.duration.seconds
        if position.isFinite && position >= 0 { result["position"] = position }
        if duration.isFinite && duration >= 0 { result["duration"] = duration }
        let buffered = item.loadedTimeRanges.map { CMTimeRangeGetEnd($0.timeRangeValue).seconds }.filter { $0.isFinite && $0 >= 0 }.max()
        if let buffered { result["buffered"] = buffered }
        return result
    }
    private func reportTimeline() {
        let value = timeline()
        if let duration = value["duration"], duration != lastDuration { lastDuration = duration; emit?("duration", duration) }
        if let position = value["position"] { emit?("time", position) }
        if !value.isEmpty { emit?("progress", value) }
    }
    private func reportBuffering() {
        guard let player, let item = player.currentItem else { return }
        let buffering = !paused && !reachedEnd && (player.timeControlStatus == .waitingToPlayAtSpecifiedRate || item.isPlaybackBufferEmpty)
        if buffering != lastBuffering { lastBuffering = buffering; emit?("buffer", buffering) }
    }
    private func reportFailure(_ message: String) {
        guard !failed else { return }; failed = true; player?.pause(); JadixAudioSession.shared.release(sessionToken)
        emit?("playing", false); emit?("error", String(message.prefix(4096)))
    }
    private func seek(to seconds: Double) {
        guard seconds.isFinite, seconds >= 0, let player else { return }
        let token = generation, operation = UUID(); seekGeneration = operation; seeking = true
        player.currentItem?.cancelPendingSeeks()
        player.seek(to: CMTime(seconds: seconds, preferredTimescale: 60_000), toleranceBefore: .zero, toleranceAfter: .zero) { [weak self] completed in
            DispatchQueue.main.async {
                guard let self, self.generation == token, self.seekGeneration == operation else { return }
                self.seeking = false
                if completed { self.reachedEnd = false; self.reportTimeline() }
                self.syncPlayback()
            }
        }
    }
    private func syncPlayback() {
        let visible = jadixSurfaceVisible(view)
        if failed || paused || reachedEnd || seeking || !JadixAudioSession.shared.canPlay || (requiresWindow && view.window == nil) || (!audio && !visible) {
            player?.pause(); JadixAudioSession.shared.release(sessionToken)
        } else {
            do { try JadixAudioSession.shared.acquire(sessionToken) }
            catch { reportFailure(error.localizedDescription); return }
            if player?.timeControlStatus != .waitingToPlayAtSpecifiedRate && player?.rate != rate { player?.playImmediately(atRate: rate) }
        }
    }
    func dispose() {
        generation = UUID(); seekGeneration = UUID()
        visibility?.invalidate(); visibility = nil; player?.pause(); player?.currentItem?.cancelPendingSeeks()
        if let clock { player?.removeTimeObserver(clock) }; clock = nil
        for notification in notifications { NotificationCenter.default.removeObserver(notification) }; notifications.removeAll()
        for observation in observations { observation.invalidate() }; observations.removeAll()
        player?.replaceCurrentItem(with: nil)
#if os(iOS)
        controller.player = nil
#else
        view.player = nil
#endif
        player = nil; source = nil
        JadixAudioSession.shared.release(sessionToken)
        reachedEnd = false; seeking = false; failed = false; lastBuffering = nil; lastDuration = nil
    }
}


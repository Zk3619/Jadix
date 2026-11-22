import Foundation
import AVFoundation

// The transport bounds encoded data before decoding. A private serial delegate
// also rejects redirects outside HTTPS and cancels immediately at the byte cap.
private final class JadixAudioDownload: NSObject, URLSessionDataDelegate {
    private var data = Data()
    private var session: URLSession?
    private var task: URLSessionDataTask?
    private var completion: ((Result<Data, Error>) -> Void)?
    init(url: URL, completion: @escaping (Result<Data, Error>) -> Void) {
        super.init(); self.completion = completion
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 30; config.timeoutIntervalForResource = 60
        config.urlCache = nil; config.httpCookieStorage = nil; config.httpShouldSetCookies = false
        let queue = OperationQueue(); queue.maxConcurrentOperationCount = 1
        let session = URLSession(configuration: config, delegate: self, delegateQueue: queue)
        self.session = session; task = session.dataTask(with: url); task?.resume()
    }
    func cancel() { task?.cancel() }
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        guard request.url?.scheme == "https", request.url?.user == nil, request.url?.password == nil,
              request.url?.host == task.originalRequest?.url?.host else {
            completionHandler(nil); task.cancel(); return
        }
        completionHandler(request)
    }
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode), response.expectedContentLength <= 8 * 1024 * 1024 else { completionHandler(.cancel); return }
        completionHandler(.allow)
    }
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive chunk: Data) {
        guard data.count + chunk.count <= 8 * 1024 * 1024 else { dataTask.cancel(); return }; data.append(chunk)
    }
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        let completion = self.completion; self.completion = nil
        let result: Result<Data, Error> = error.map { .failure($0) } ?? .success(data)
        self.task = nil; self.session = nil; session.finishTasksAndInvalidate()
        DispatchQueue.main.async { completion?(result) }
    }
}

@MainActor final class JadixAudioService {
    var event: ((UInt32, [String: Any]) -> Void)?
    private struct Sound { let buffer: AVAudioPCMBuffer; let bytes: Int }
    private final class Voice {
        let player = AVAudioPlayerNode(), speed = AVAudioUnitVarispeed()
        let sound: UInt32
        init(sound: UInt32) { self.sound = sound }
    }
    private final class Stream {
        let media = JadixHostedMedia()
        var values: [String: Any]
        var temporary: URL?
        init(source: URL, settings: [String: Any]) {
            values = settings; values["source"] = source.absoluteString; values["audio"] = true
            values["paused"] = true; media.requiresWindow = false
        }
        func update() { media.update(values) }
        func dispose() { media.dispose(); if let temporary { try? FileManager.default.removeItem(at: temporary) } }
    }
    private let engine = AVAudioEngine()
    private var sounds: [UInt32: Sound] = [:]
    private var streams: [UInt32: Stream] = [:]
    private var generations: [UInt32: UUID] = [:]
    private var downloads: [UInt32: JadixAudioDownload] = [:]
    private var voices: [Voice] = []
    private var configuration: NSObjectProtocol?
    private let sessionToken = UUID()
    private var sessionObserver: NSObjectProtocol?
    private var pendingStreams: [UInt32: [String: Any]] = [:]
    private var processingBytes = 0
    private let worker: OperationQueue = { let queue = OperationQueue(); queue.maxConcurrentOperationCount = 2; queue.qualityOfService = .userInitiated; return queue }()
    private var disposed = false
    init() {
        sessionObserver = NotificationCenter.default.addObserver(forName: .jadixAudioSessionChanged, object: nil, queue: .main) { [weak self] _ in
            DispatchQueue.main.async { self?.recoverDevice() }
        }
        configuration = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main) { [weak self] _ in
            DispatchQueue.main.async { self?.recoverDevice() }
        }
    }
    func handle(_ commands: [[String: Any]]) {
        guard !disposed else { return }
        for command in commands {
            guard let raw = command["id"] as? NSNumber, raw.uint64Value <= UInt64(UInt32.max) else { continue }
            let id = raw.uint32Value
            switch command["action"] as? String {
            case "load": load(id, source: command["source"] as? [String: Any] ?? [:], settings: nil)
            case "open": load(id, source: command["source"] as? [String: Any] ?? [:], settings: command["settings"] as? [String: Any] ?? [:])
            case "play": play(id, volume: (command["volume"] as? NSNumber)?.floatValue ?? 1, rate: (command["rate"] as? NSNumber)?.floatValue ?? 1)
            case "stop": stop(id)
            case "unload", "close": remove(id)
            case "preload": streams[id]?.update()
            case "resume": JadixAudioSession.shared.requestPlayback(); updateStream(id) { $0["paused"] = false }
            case "pause": updateStream(id) { $0["paused"] = true }
            case "seek": updateStream(id) {
                $0["seek"] = command["position"]
                $0["seek_revision"] = ($0["seek_revision"] as? UInt64 ?? 0) &+ 1
            }
            case "set": if let settings = command["settings"] as? [String: Any] {
                updateStream(id) { values in for (key, value) in settings { values[key] = value } }
            }
            default: fail(id, "Unknown audio command")
            }
        }
    }
    private func updateStream(_ id: UInt32, mutate: (inout [String: Any]) -> Void) {
        if let stream = streams[id] { mutate(&stream.values); stream.update() }
        else if var values = pendingStreams[id] { mutate(&values); pendingStreams[id] = values }
    }
    private func fail(_ id: UInt32, _ message: String) { event?(id, ["type": "failed", "message": String(message.prefix(1024))]) }
    private func load(_ id: UInt32, source: [String: Any], settings: [String: Any]?) {
        remove(id); let token = UUID(); generations[id] = token
        guard generations.count <= 128 else { remove(id); fail(id, "Audio resource quota exceeded"); return }
        if let settings { var values = settings; values["paused"] = true; pendingStreams[id] = values }
        if let settings, source["type"] as? String != "bytes" {
            guard let url = resolve(source) else { fail(id, "Unsupported audio source"); return }
            open(id, token: token, url: url, settings: settings, temporary: false); return
        }
        let completion: (Result<Data, Error>) -> Void = { [weak self] result in
            guard let self, !self.disposed, self.generations[id] == token else { return }
            self.downloads.removeValue(forKey: id)
            switch result {
            case .failure(let error): self.fail(id, error.localizedDescription)
            case .success(let data):
                guard !data.isEmpty, data.count <= 8 * 1024 * 1024 else { self.fail(id, "Audio encoded quota exceeded"); return }
                guard self.processingBytes + data.count <= 32 * 1024 * 1024 else { self.fail(id, "Audio decoding queue quota exceeded"); return }
                self.processingBytes += data.count
                self.worker.addOperation {
                    defer { DispatchQueue.main.async { [weak self] in self?.processingBytes -= data.count } }
                    let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".audio")
                    do {
                        try data.write(to: file, options: [.atomic])
                        if let settings {
                            DispatchQueue.main.async { [weak self] in
                                guard let self, !self.disposed, self.generations[id] == token else { try? FileManager.default.removeItem(at: file); return }
                                self.open(id, token: token, url: file, settings: settings, temporary: true)
                            }
                            return
                        }
                        defer { try? FileManager.default.removeItem(at: file) }
                        let audio = try AVAudioFile(forReading: file, commonFormat: .pcmFormatFloat32, interleaved: false)
                        let format = audio.processingFormat, frames = audio.length
                        let duration = Double(frames) / format.sampleRate
                        let (samples, sampleOverflow) = frames.multipliedReportingOverflow(by: Int64(format.channelCount))
                        let (bytes, byteOverflow) = samples.multipliedReportingOverflow(by: 4)
                        guard format.sampleRate.isFinite, format.sampleRate > 0, frames > 0,
                              !sampleOverflow, !byteOverflow, frames <= Int64(UInt32.max),
                              duration <= 5, bytes > 0, bytes <= 16 * 1024 * 1024,
                              let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frames)) else { throw URLError(.dataLengthExceedsMaximum) }
                        try audio.read(into: buffer)
                        guard buffer.frameLength == buffer.frameCapacity else { throw URLError(.cannotDecodeContentData) }
                        DispatchQueue.main.async { [weak self] in
                            guard let self, !self.disposed, self.generations[id] == token else { return }
                            guard self.sounds.count < 64, self.sounds.values.reduce(0, { $0 + $1.bytes }) + Int(bytes) <= 16 * 1024 * 1024 else { self.fail(id, "Audio decoded quota exceeded"); return }
                            self.sounds[id] = Sound(buffer: buffer, bytes: Int(bytes))
                            self.event?(id, ["type": "loaded", "duration": duration, "decoded_bytes": bytes])
                        }
                    } catch {
                        try? FileManager.default.removeItem(at: file)
                        DispatchQueue.main.async { [weak self] in guard let self, self.generations[id] == token else { return }; self.fail(id, error.localizedDescription) }
                    }
                }
            }
        }
        if source["type"] as? String == "bytes", let encoded = source["base64"] as? String,
           encoded.utf8.count <= 11_184_812, let data = Data(base64Encoded: encoded) { completion(.success(data)); return }
        guard let url = resolve(source) else { fail(id, "Unsupported audio source"); return }
        if url.isFileURL {
            worker.addOperation {
                let result = Result<Data, Error> {
                    guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) <= 8 * 1024 * 1024 else { throw URLError(.dataLengthExceedsMaximum) }
                    let file = try FileHandle(forReadingFrom: url); defer { try? file.close() }
                    let data = try file.read(upToCount: 8 * 1024 * 1024 + 1) ?? Data()
                    guard data.count <= 8 * 1024 * 1024 else { throw URLError(.dataLengthExceedsMaximum) }; return data
                }
                DispatchQueue.main.async { completion(result) }
            }
        } else {
            guard downloads.count < 4 else { fail(id, "Audio download quota exceeded"); return }
            downloads[id] = JadixAudioDownload(url: url, completion: completion)
        }
    }
    private func resolve(_ source: [String: Any]) -> URL? {
        if source["type"] as? String == "file", let path = source["path"] as? String, path.hasPrefix("/") { return URL(fileURLWithPath: path) }
        guard source["type"] as? String == "url", let raw = source["url"] as? String, let url = URL(string: raw) else { return nil }
        if let bundle = jadixBundleResource(url) { return bundle }
        return url.scheme == "https" && url.host != nil && url.user == nil && url.password == nil ? url : nil
    }
    private func open(_ id: UInt32, token: UUID, url: URL, settings: [String: Any], temporary: Bool) {
        guard streams.count < 64 else { if temporary { try? FileManager.default.removeItem(at: url) }; fail(id, "Audio player quota exceeded"); return }
        let stream = Stream(source: url, settings: settings)
        if let pending = pendingStreams.removeValue(forKey: id) { for (key, value) in pending { stream.values[key] = value } }
        if temporary { stream.temporary = url }; streams[id] = stream
        stream.media.emit = { [weak self] name, value in
            guard let self, !self.disposed, self.generations[id] == token else { return }
            var event: [String: Any]
            switch name {
            case "load": event = ["type": "load", "duration": (value as? [String: Double])?["duration"] as Any? ?? NSNull()]
            case "progress":
                guard let values = value as? [String: Double], let position = values["position"] else { return }
                event = ["type": "progress", "position": position, "duration": values["duration"] as Any? ?? NSNull(), "buffered": values["buffered"] as Any? ?? NSNull()]
            case "playing": event = ["type": value as? Bool == true ? "playing" : "paused"]
            case "buffer": event = ["type": "buffer", "buffering": value]
            case "ended": event = ["type": "ended"]
            case "error": event = ["type": "failed", "message": value]
            default: return
            }
            self.event?(id, event)
        }
        stream.update()
    }
    private func play(_ id: UInt32, volume: Float, rate: Float) {
        guard let sound = sounds[id], volume.isFinite, rate.isFinite, (0...1).contains(volume), (0.25...4).contains(rate) else { fail(id, "Sound is not decoded"); return }
        JadixAudioSession.shared.requestPlayback()
        guard JadixAudioSession.shared.canPlay else { fail(id, "Audio session is interrupted"); return }
        do { try JadixAudioSession.shared.acquire(sessionToken) }
        catch { fail(id, error.localizedDescription); return }
        if voices.count >= 32 { finish(voices[0]) }
        let voice = Voice(sound: id); voices.append(voice)
        engine.attach(voice.player); engine.attach(voice.speed)
        engine.connect(voice.player, to: voice.speed, format: sound.buffer.format)
        engine.connect(voice.speed, to: engine.mainMixerNode, format: sound.buffer.format)
        voice.player.volume = volume; voice.speed.rate = rate
        do { if !engine.isRunning { try engine.start() } }
        catch { finish(voice); fail(id, error.localizedDescription); return }
        voice.player.scheduleBuffer(sound.buffer, completionCallbackType: .dataPlayedBack) { [weak self, weak voice] _ in
            DispatchQueue.main.async { guard let self, let voice, self.voices.contains(where: { $0 === voice }) else { return }; self.finish(voice) }
        }
        voice.player.play(); event?(id, ["type": "playing"])
    }
    private func finish(_ voice: Voice) {
        voice.player.stop(); engine.disconnectNodeOutput(voice.player); engine.disconnectNodeOutput(voice.speed)
        engine.detach(voice.player); engine.detach(voice.speed); voices.removeAll { $0 === voice }
        if voices.isEmpty { engine.stop(); JadixAudioSession.shared.release(sessionToken) }
        if !voices.contains(where: { $0.sound == voice.sound }) { event?(voice.sound, ["type": "ended"]) }
    }
    private func stop(_ id: UInt32) {
        let active = voices.filter { $0.sound == id }; for voice in active { finish(voice) }
        if !active.isEmpty { event?(id, ["type": "paused"]) }
    }
    private func remove(_ id: UInt32) {
        pendingStreams.removeValue(forKey: id)
        generations.removeValue(forKey: id); downloads.removeValue(forKey: id)?.cancel()
        stop(id); sounds.removeValue(forKey: id); streams.removeValue(forKey: id)?.dispose()
    }
    private func recoverDevice() {
        for voice in voices { finish(voice) }; engine.stop(); engine.reset()
        // Stream players use AVPlayer's independent device recovery.
    }
    func dispose() {
        disposed = true; event = nil
        for id in Array(generations.keys) { remove(id) }
        engine.stop(); if let configuration { NotificationCenter.default.removeObserver(configuration) }; configuration = nil
        if let sessionObserver { NotificationCenter.default.removeObserver(sessionObserver) }; sessionObserver = nil
        JadixAudioSession.shared.release(sessionToken)
    }
}

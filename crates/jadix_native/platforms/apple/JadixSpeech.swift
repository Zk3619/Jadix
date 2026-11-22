import Foundation
import AVFoundation
import Speech

@MainActor final class JadixSpeechService {
    var event: ((UInt32, [String: Any]) -> Void)?
    private var id: UInt32?
    private var generation = UUID()
    private var engine: AVAudioEngine?
    private var recognizer: SFSpeechRecognizer?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var timeout: Timer?
    private var stopping = false
    private var hasTap = false
    private var configuration: NSObjectProtocol?
    private let sessionToken = UUID()
    private var sessionObserver: NSObjectProtocol?
    func handle(_ command: [String: Any]) {
        guard let number = command["id"] as? NSNumber, number.uint64Value <= UInt64(UInt32.max) else { return }
        let session = number.uint32Value
        switch command["action"] as? String {
        case "start": start(session, options: command["request"] as? [String: Any] ?? [:])
        case "abort": if id == session { end(nil) }
        case "stop": if id == session && !stopping {
            stopping = true
            guard request != nil else { end(nil); return }
            captureStopped(); request?.endAudio(); task?.finish()
            timeout?.invalidate(); let token = generation
            timeout = Timer.scheduledTimer(withTimeInterval: 5, repeats: false) { [weak self] _ in guard let self, self.generation == token else { return }; self.end(nil) }
        }
        default: break
        }
    }
    private func start(_ session: UInt32, options: [String: Any]) {
        end(nil); id = session; generation = UUID(); stopping = false
        let token = generation
        // Missing usage metadata must be reported as a normal failure, never an
        // OS privacy exception terminating the embedding application.
        guard Bundle.main.object(forInfoDictionaryKey: "NSSpeechRecognitionUsageDescription") as? String != nil,
              Bundle.main.object(forInfoDictionaryKey: "NSMicrophoneUsageDescription") as? String != nil else { end("not-allowed"); return }
        let locale = (options["lang"] as? String).map { Locale(identifier: $0) } ?? Locale.current
        guard SFSpeechRecognizer.supportedLocales().contains(where: { $0.identifier.replacingOccurrences(of: "_", with: "-").lowercased() == locale.identifier.replacingOccurrences(of: "_", with: "-").lowercased() }),
              let recognizer = SFSpeechRecognizer(locale: locale) else { end("language-not-supported"); return }
        self.recognizer = recognizer
        SFSpeechRecognizer.requestAuthorization { [weak self] status in
            DispatchQueue.main.async {
                guard let self, self.id == session, self.generation == token else { return }
                guard status == .authorized else { self.end("not-allowed"); return }
                AVCaptureDevice.requestAccess(for: .audio) { [weak self] allowed in
                    DispatchQueue.main.async {
                        guard let self, self.id == session, self.generation == token else { return }
                        guard allowed else { self.end("not-allowed"); return }
                        self.listen(interim: options["interim"] as? Bool ?? true, token: token)
                    }
                }
            }
        }
    }
    private func listen(interim: Bool, token: UUID) {
        guard let id, let recognizer, recognizer.isAvailable else { end("unavailable"); return }
        do { try JadixAudioSession.shared.acquire(sessionToken, recording: true) }
        catch { end("audio-capture"); return }
        sessionObserver = NotificationCenter.default.addObserver(forName: .jadixAudioSessionChanged, object: nil, queue: .main) { [weak self] _ in
            DispatchQueue.main.async { self?.end("audio-capture") }
        }
        let engine = AVAudioEngine(), request = SFSpeechAudioBufferRecognitionRequest()
        self.engine = engine; self.request = request; request.shouldReportPartialResults = interim
        let input = engine.inputNode, format = engine.inputNode.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { end("audio-capture"); return }
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in request.append(buffer) }; hasTap = true
        configuration = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main) { [weak self] _ in
            DispatchQueue.main.async { guard let self, self.generation == token else { return }; self.end("audio-capture") }
        }
        do { engine.prepare(); try engine.start() }
        catch { end("audio-capture"); return }
        event?(id, ["type": "start"])
        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            DispatchQueue.main.async {
                guard let self, self.id == id, self.generation == token else { return }
                if let result {
                    let text = result.bestTranscription.formattedString
                    guard text.utf8.count <= 64 * 1024 else { self.end("failed"); return }
                    if interim || result.isFinal { self.event?(id, ["type": "result", "text": text, "is_final": result.isFinal]) }
                    if result.isFinal { self.end(nil); return }
                }
                if let error = error as NSError? {
                    let kind: String
                    if error.domain == NSURLErrorDomain { kind = "network" }
                    else if error.code == 1110 { kind = "no-speech" }
                    else if error.code == 1101 || error.code == 1107 { kind = "unavailable" }
                    else { kind = "failed" }
                    self.end(kind)
                }
            }
        }
        timeout = Timer.scheduledTimer(withTimeInterval: 60, repeats: false) { [weak self] _ in
            guard let self, self.generation == token else { return }
            self.handle(["action": "stop", "id": id])
        }
    }
    private func captureStopped() {
        engine?.stop()
        if hasTap { engine?.inputNode.removeTap(onBus: 0); hasTap = false }
        if let configuration { NotificationCenter.default.removeObserver(configuration) }; configuration = nil
        if let sessionObserver { NotificationCenter.default.removeObserver(sessionObserver) }; sessionObserver = nil
        JadixAudioSession.shared.release(sessionToken)
    }
    private func end(_ error: String?) {
        let session = id; id = nil; generation = UUID()
        timeout?.invalidate(); timeout = nil; captureStopped()
        request?.endAudio(); task?.cancel(); task = nil; request = nil; engine = nil; recognizer = nil
        if let session { event?(session, ["type": "end", "error": error as Any? ?? NSNull()]) }
    }
    func dispose() { event = nil; end(nil) }
}

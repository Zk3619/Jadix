import Foundation
import AVFoundation

// One coordinator for all roots, players and microphone users in an embedding
// process. Releasing one root must not deactivate another root's audio session.
@MainActor final class JadixAudioSession {
    static let shared = JadixAudioSession()
    private var clients: [UUID: Bool] = [:]
    private(set) var interrupted = false
    private var resumeBlocked = false
    var canPlay: Bool { !interrupted && !resumeBlocked }
    func requestPlayback() { if !interrupted { resumeBlocked = false } }
    private var observers: [NSObjectProtocol] = []
    private init() {
        #if os(iOS)
        observers.append(NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] event in
            MainActor.assumeIsolated {
                guard let self, let raw = event.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt else { return }
                self.interrupted = raw == AVAudioSession.InterruptionType.began.rawValue
                if !self.interrupted {
                    let options = (event.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt) ?? 0
                    self.resumeBlocked = !AVAudioSession.InterruptionOptions(rawValue: options).contains(.shouldResume)
                    if self.canPlay, !self.clients.isEmpty { self.restore() }
                }
                NotificationCenter.default.post(name: .jadixAudioSessionChanged, object: self)
            }
        })
        observers.append(NotificationCenter.default.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { [weak self] event in
            MainActor.assumeIsolated {
                guard let self else { return }
                if let raw = event.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
                   AVAudioSession.RouteChangeReason(rawValue: raw) == .oldDeviceUnavailable { self.resumeBlocked = true }
                NotificationCenter.default.post(name: .jadixAudioSessionChanged, object: self)
            }
        })
        observers.append(NotificationCenter.default.addObserver(forName: AVAudioSession.mediaServicesWereResetNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }; self.interrupted = false
                if self.canPlay, !self.clients.isEmpty { self.restore() }
                NotificationCenter.default.post(name: .jadixAudioSessionChanged, object: self)
            }
        })
        #endif
    }
    func acquire(_ token: UUID, recording: Bool = false) throws {
        guard clients[token] != recording else { return }
        let previous = clients[token]; clients[token] = recording
        do { try configure() } catch { clients[token] = previous; throw error }
    }
    func release(_ token: UUID) {
        guard clients.removeValue(forKey: token) != nil else { return }
        #if os(iOS)
        if clients.isEmpty { try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation) }
        else { try? configure() }
        #endif
    }
    private func restore() {
        do { try configure() } catch { resumeBlocked = true }
    }
    private func configure() throws {
        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
        let recording = clients.values.contains(true)
        try session.setCategory(recording ? .playAndRecord : .playback, mode: recording ? .measurement : .default,
                                options: recording ? [.mixWithOthers, .defaultToSpeaker, .allowBluetooth] : [.mixWithOthers])
        if canPlay { try session.setActive(true) }
        #endif
    }
}
extension Notification.Name { static let jadixAudioSessionChanged = Notification.Name("JadixAudioSessionChanged") }

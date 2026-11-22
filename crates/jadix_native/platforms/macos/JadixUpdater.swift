import AppKit
import Foundation

private struct JadixUpdateError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

// Only binary transfers live off the main thread. The shared Rust state machine
// receives bounded chunks on the main thread and owns trust/version decisions.
private final class JadixUpdateTransport: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    private struct Transfer {
        let task: URLSessionDataTask
        let limit: Int64
        let file: URL?
        let handle: FileHandle?
        let chunk: ((Data) -> Bool)?
        let completion: (Result<Data, Error>) -> Void
        var data = Data()
        var received: Int64 = 0
        var redirects = 0
    }
    private let lock = NSLock()
    private var transfers: [Int: Transfer] = [:]
    private lazy var session: URLSession = {
        let config = URLSessionConfiguration.ephemeral
        config.httpCookieStorage = nil; config.urlCredentialStorage = nil; config.urlCache = nil
        config.timeoutIntervalForRequest = 60; config.timeoutIntervalForResource = 3600
        let queue = OperationQueue(); queue.maxConcurrentOperationCount = 1
        return URLSession(configuration: config, delegate: self, delegateQueue: queue)
    }()
    private func valid(_ url: URL) -> Bool {
        url.scheme?.lowercased() == "https" && url.host != nil && url.user == nil && url.password == nil && url.fragment == nil
    }
    func get(_ raw: String, limit: Int64, file: URL? = nil, chunk: ((Data) -> Bool)? = nil,
             completion: @escaping (Result<Data, Error>) -> Void) -> URLSessionDataTask? {
        do {
            guard let url = URL(string: raw), valid(url), limit > 0 else { throw JadixUpdateError(message: "Invalid update URL") }
            var handle: FileHandle?
            if let file {
                guard FileManager.default.createFile(atPath: file.path, contents: nil, attributes: [.posixPermissions: 0o600]) else {
                    throw JadixUpdateError(message: "Cannot create private update download")
                }
                handle = try FileHandle(forWritingTo: file)
            }
            let task = session.dataTask(with: URLRequest(url: url))
            lock.lock(); transfers[task.taskIdentifier] = Transfer(task: task, limit: limit, file: file, handle: handle, chunk: chunk, completion: completion); lock.unlock()
            task.resume(); return task
        } catch { completion(.failure(error)); return nil }
    }
    func cancel(_ task: URLSessionDataTask) { finish(task, .failure(URLError(.cancelled))) }
    private func finish(_ task: URLSessionTask, _ result: Result<Data, Error>) {
        lock.lock(); let transfer = transfers.removeValue(forKey: task.taskIdentifier); lock.unlock()
        guard let transfer else { return }
        do { try transfer.handle?.synchronize(); try transfer.handle?.close() }
        catch { task.cancel(); if let file = transfer.file { try? FileManager.default.removeItem(at: file) }; transfer.completion(.failure(error)); return }
        if case .failure = result { task.cancel(); if let file = transfer.file { try? FileManager.default.removeItem(at: file) } }
        transfer.completion(result)
    }
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        lock.lock(); let transfer = transfers[dataTask.taskIdentifier]; lock.unlock()
        guard let transfer, let response = response as? HTTPURLResponse, (200..<300).contains(response.statusCode),
              let url = response.url, valid(url), response.expectedContentLength <= transfer.limit else {
            finish(dataTask, .failure(JadixUpdateError(message: "Update HTTP status or byte limit rejected"))); completionHandler(.cancel); return
        }
        completionHandler(.allow)
    }
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        lock.lock()
        guard var transfer = transfers[dataTask.taskIdentifier] else { lock.unlock(); return }
        guard Int64(data.count) <= transfer.limit - transfer.received else {
            lock.unlock(); finish(dataTask, .failure(JadixUpdateError(message: "Update response exceeds byte limit"))); return
        }
        transfer.received += Int64(data.count)
        do {
            if let handle = transfer.handle { try handle.write(contentsOf: data) } else { transfer.data.append(data) }
            transfers[dataTask.taskIdentifier] = transfer; lock.unlock()
        } catch { lock.unlock(); finish(dataTask, .failure(error)); return }
        if let chunk = transfer.chunk {
            for offset in stride(from: 0, to: data.count, by: 64 * 1024) {
                guard chunk(data.subdata(in: offset..<min(offset + 64 * 1024, data.count))) else {
                    finish(dataTask, .failure(JadixUpdateError(message: "Update transfer cancelled or verification failed"))); return
                }
            }
        }
    }
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        lock.lock()
        guard var transfer = transfers[task.taskIdentifier], transfer.redirects < 5, let url = request.url, valid(url) else {
            lock.unlock(); finish(task, .failure(JadixUpdateError(message: "Update redirect rejected"))); completionHandler(nil); return
        }
        transfer.redirects += 1; transfers[task.taskIdentifier] = transfer; lock.unlock()
        var redirected = request; redirected.setValue(nil, forHTTPHeaderField: "Authorization"); redirected.setValue(nil, forHTTPHeaderField: "Cookie")
        completionHandler(redirected)
    }
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        lock.lock(); let transfer = transfers[task.taskIdentifier]; lock.unlock()
        guard let transfer else { return }
        finish(task, error.map { .failure($0) } ?? .success(transfer.data))
    }
}

/// An application service: closing/replacing a view never cancels a staged update.
@MainActor
final class JadixApplicationUpdater {
    static let shared = JadixApplicationUpdater()
    private let transport = JadixUpdateTransport()
    private var started = false
    private var timer: Timer?
    private var observer: NSObjectProtocol?
    private var generation: UInt32 = 0
    private var tasks: [URLSessionDataTask] = []
    private var downloadRoot: URL?
    private var download: URL?
    private var staged: Staged?
    private var installOnQuit = false
    private var restart = false
    private struct Staged { let root: URL; let app: URL; let installed: URL }

    func start() {
        guard !started else { return }; started = true
        let bundle = Bundle.main; let app = bundle.bundleURL.resolvingSymlinksInPath()
        var configJSON = "null"
        let packaged = app.pathExtension == "app" && bundle.executableURL?.resolvingSymlinksInPath().deletingLastPathComponent() == app.appendingPathComponent("Contents/MacOS")
        if packaged, let resource = bundle.url(forResource: "jadix-updater", withExtension: "json"),
           let data = try? Data(contentsOf: resource, options: .mappedIfSafe), data.count <= 64 * 1024,
           var config = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
           config["id"] as? String == bundle.bundleIdentifier,
           let version = bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String {
            config["version"] = version
            if let encoded = try? JSONSerialization.data(withJSONObject: config), let json = String(data: encoded, encoding: .utf8) { configJSON = json }
        }
        #if arch(arm64)
        let platform = "macos-aarch64"
        #else
        let platform = "macos-x86_64"
        #endif
        let configured = configJSON.withCString { json in platform.withCString { jadix_native_updater_configure(json, $0, packaged) } }
        if !configured { NSLog("Jadix: packaged application update configuration was rejected") }
        observer = NotificationCenter.default.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.finishOnQuit() }
        }
        let timer = Timer(timeInterval: 0.1, repeats: true) { [weak self] _ in MainActor.assumeIsolated { self?.poll() } }
        self.timer = timer; RunLoop.main.add(timer, forMode: .common); RunLoop.main.add(timer, forMode: .modalPanel)
    }
    private func event(_ id: UInt32, _ value: [String: Any]) {
        guard id == generation, let data = try? JSONSerialization.data(withJSONObject: value), let json = String(data: data, encoding: .utf8) else { return }
        _ = json.withCString { jadix_native_updater_event(id, $0) }
    }
    private func failed(_ id: UInt32, _ error: Error) { event(id, ["type": "failed", "message": String(error.localizedDescription.prefix(1024))]) }
    private func poll() {
        guard let pointer = jadix_native_updater_commands() else { return }
        defer { jadix_native_string_free(pointer) }
        guard let data = String(cString: pointer).data(using: .utf8), let commands = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] else { return }
        for command in commands {
            guard let id = (command["id"] as? NSNumber)?.uint32Value, let action = command["action"] as? String else { continue }
            if action == "cancel" { if id == generation { cancel() }; continue }
            if action == "manifest" || action == "download" { generation = id }
            guard id == generation else { continue }
            switch action {
            case "manifest":
                guard let url = command["url"] as? String else { continue }
                fetch(id, url, limit: 1024 * 1024) { [weak self] bytes in
                    guard let self else { return }
                    if let signatureURL = command["signature_url"] as? String {
                        self.fetch(id, signatureURL, limit: 4096) { signature in
                            guard let text = String(data: signature, encoding: .utf8) else { self.failed(id, JadixUpdateError(message: "Invalid update signature encoding")); return }
                            self.event(id, ["type": "manifest", "bytes": Array(bytes), "signature": text])
                        }
                    } else { self.event(id, ["type": "manifest", "bytes": Array(bytes)]) }
                }
            case "notes":
                if let file = command["file"] as? [String: Any], let url = file["url"] as? String {
                    fetch(id, url, limit: 256 * 1024) { [weak self] bytes in self?.event(id, ["type": "notes", "bytes": Array(bytes)]) }
                }
            case "download": beginDownload(id, command)
            case "stage":
                if let identity = command["application_id"] as? String, let version = command["version"] as? String { stage(id, identity: identity, version: version) }
            case "install":
                guard staged != nil else { failed(id, JadixUpdateError(message: "Update staging is unavailable")); continue }
                installOnQuit = true; restart = command["restart"] as? Bool ?? false
                event(id, ["type": "scheduled"])
                if restart { DispatchQueue.main.async { NSApplication.shared.terminate(nil) } }
            default: break
            }
        }
    }
    private func fetch(_ id: UInt32, _ url: String, limit: Int64, success: @escaping (Data) -> Void) {
        if let task = transport.get(url, limit: limit, completion: { [weak self] result in
            DispatchQueue.main.async {
                guard let self, self.generation == id else { return }
                switch result { case .success(let data): success(data); case .failure(let error): self.failed(id, error) }
            }
        }) { tasks.append(task) }
    }
    private func beginDownload(_ id: UInt32, _ command: [String: Any]) {
        guard let url = command["url"] as? String, let limit = command["max_bytes"] as? NSNumber else { return }
        do {
            let root = FileManager.default.temporaryDirectory.appendingPathComponent("jadix-update-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
            let archive = root.appendingPathComponent("archive"); downloadRoot = root; download = archive
            if let task = transport.get(url, limit: limit.int64Value, file: archive, chunk: { bytes in
                DispatchQueue.main.sync { bytes.withUnsafeBytes { buffer in
                    jadix_native_updater_chunk(id, buffer.bindMemory(to: UInt8.self).baseAddress, buffer.count)
                } }
            }, completion: { [weak self] result in DispatchQueue.main.async {
                guard let self, self.generation == id else { return }
                switch result { case .success: self.event(id, ["type": "downloaded"]); case .failure(let error): self.failed(id, error) }
            } }) { tasks.append(task) }
        } catch { failed(id, error) }
    }
    private func cancel() {
        generation = 0
        tasks.forEach { transport.cancel($0) }; tasks.removeAll()
        if let root = downloadRoot { try? FileManager.default.removeItem(at: root) }
        if let staged { try? FileManager.default.removeItem(at: staged.root) }
        download = nil; downloadRoot = nil; staged = nil; installOnQuit = false; restart = false
    }
    private func stage(_ id: UInt32, identity: String, version: String) {
        guard let archive = download else { failed(id, JadixUpdateError(message: "Update archive is unavailable")); return }
        let installed = Bundle.main.bundleURL.resolvingSymlinksInPath()
        DispatchQueue.global(qos: .utility).async { [weak self] in
            var root: URL?
            do {
                guard installed.pathExtension == "app" else { throw JadixUpdateError(message: "Application updates require a packaged app") }
                let scratch = installed.deletingLastPathComponent().appendingPathComponent(".jadix-update-\(UUID().uuidString)", isDirectory: true)
                try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700]); root = scratch
                let payload = scratch.appendingPathComponent("payload", isDirectory: true)
                let errorPointer = archive.path.withCString { source in payload.path.withCString { jadix_native_updater_unpack(source, $0) } }
                if let errorPointer { let message = String(cString: errorPointer); jadix_native_string_free(errorPointer); throw JadixUpdateError(message: message) }
                let candidates = try FileManager.default.contentsOfDirectory(at: payload, includingPropertiesForKeys: [.isSymbolicLinkKey, .isDirectoryKey]).filter { $0.pathExtension == "app" }
                guard candidates.count == 1, let app = candidates.first,
                      (try app.resourceValues(forKeys: [.isSymbolicLinkKey, .isDirectoryKey])).isSymbolicLink == false,
                      let bundle = Bundle(url: app), bundle.bundleIdentifier == identity,
                      bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String == version,
                      let executable = bundle.executableURL?.resolvingSymlinksInPath(),
                      executable.path.hasPrefix(app.resolvingSymlinksInPath().path + "/"), FileManager.default.isExecutableFile(atPath: executable.path) else {
                    throw JadixUpdateError(message: "Update bundle identity, version or executable mismatch")
                }
                let verify = Process(); verify.executableURL = URL(fileURLWithPath: "/usr/bin/codesign"); verify.arguments = ["--verify", "--deep", "--strict", app.path]
                verify.standardOutput = FileHandle.nullDevice; verify.standardError = FileHandle.nullDevice
                try verify.run(); verify.waitUntilExit()
                guard verify.terminationStatus == 0 else { throw JadixUpdateError(message: "Update code signature verification failed") }
                let staged = Staged(root: scratch, app: app, installed: installed)
                DispatchQueue.main.async {
                    guard let self, self.generation == id else { try? FileManager.default.removeItem(at: scratch); return }
                    self.staged = staged; self.event(id, ["type": "staged"])
                }
            } catch {
                if let root { try? FileManager.default.removeItem(at: root) }
                DispatchQueue.main.async { self?.failed(id, error) }
            }
        }
    }
    private func finishOnQuit() {
        guard installOnQuit, let staged else { cancel(); return }
        let manager = FileManager.default
        let backup = staged.root.appendingPathComponent("previous.app", isDirectory: true)
        do {
            try manager.moveItem(at: staged.installed, to: backup)
            do { try manager.moveItem(at: staged.app, to: staged.installed) }
            catch { try manager.moveItem(at: backup, to: staged.installed); throw error }
            if restart {
                // Arguments are positional data. Relaunch only after the old process exits.
                let helper = Process(); helper.executableURL = URL(fileURLWithPath: "/bin/sh")
                helper.arguments = ["-c", "i=0; while kill -0 \"$1\" 2>/dev/null; do i=$((i+1)); [ \"$i\" -lt 120 ] || exit 1; sleep 1; done; exec /usr/bin/open -n \"$2\"", "jadix-relaunch", String(ProcessInfo.processInfo.processIdentifier), staged.installed.path]
                helper.standardInput = FileHandle.nullDevice; helper.standardOutput = FileHandle.nullDevice; helper.standardError = FileHandle.nullDevice
                do { try helper.run() }
                catch {
                    // A failed relaunch must leave the previously installed app usable.
                    try manager.moveItem(at: staged.installed, to: staged.app)
                    try manager.moveItem(at: backup, to: staged.installed)
                    throw error
                }
            }
            try? manager.removeItem(at: staged.root)
            if let root = downloadRoot { try? manager.removeItem(at: root) }
        } catch { NSLog("Jadix application update failed: %@", error.localizedDescription) }
    }
}

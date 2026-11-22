import Foundation

// A session belongs to one mounted root. Foundation performs TLS verification,
// decompression and protocol cache validation; no shell process handles requests.
final class JadixFetchService: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    private struct Transfer {
        let id: UInt32
        let task: URLSessionDataTask
        let limit: Int
        var received = 0
        var redirects = 0
    }
    private let lock = NSLock()
    private var transfers: [Int: Transfer] = [:]
    private var closed = false
    var event: (@Sendable (UInt32, [String: Any]) -> Void)?
    private lazy var session: URLSession = {
        let config = URLSessionConfiguration.ephemeral
        config.urlCache = URLCache(memoryCapacity: 16 * 1024 * 1024, diskCapacity: 0)
        config.requestCachePolicy = .useProtocolCachePolicy
        config.httpCookieStorage = nil
        config.urlCredentialStorage = nil
        config.httpMaximumConnectionsPerHost = 6
        let queue = OperationQueue(); queue.maxConcurrentOperationCount = 1
        return URLSession(configuration: config, delegate: self, delegateQueue: queue)
    }()
    private func validURL(_ url: URL) -> Bool {
        ["http", "https"].contains(url.scheme?.lowercased() ?? "") && url.host != nil && url.user == nil && url.password == nil
    }
    func handle(_ command: [String: Any]) {
        guard let id = (command["id"] as? NSNumber)?.uint32Value else { return }
        if command["action"] as? String == "cancel" {
            lock.lock(); let entry = transfers.first { $0.value.id == id }
            if let entry { transfers.removeValue(forKey: entry.key) }; lock.unlock()
            entry?.value.task.cancel(); return
        }
        guard command["action"] as? String == "send", let value = command["request"] as? [String: Any],
              let rawURL = value["url"] as? String, let url = URL(string: rawURL), validURL(url),
              let method = value["method"] as? String, ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].contains(method),
              let timeout = value["timeout_ms"] as? NSNumber,
              let limit = value["max_response_bytes"] as? NSNumber else {
            event?(id, ["type": "failed", "message": "Invalid HTTP request"]); return
        }
        var request = URLRequest(url: url, timeoutInterval: timeout.doubleValue / 1000)
        request.httpMethod = method
        for header in value["headers"] as? [[String]] ?? [] where header.count == 2 { request.addValue(header[1], forHTTPHeaderField: header[0]) }
        if let bytes = value["body"] as? [UInt8], !bytes.isEmpty { request.httpBody = Data(bytes) }
        lock.lock()
        guard !closed, transfers.count < 64 else { lock.unlock(); event?(id, ["type": "failed", "message": "HTTP service is closed or full"]); return }
        let task = session.dataTask(with: request)
        transfers[task.taskIdentifier] = Transfer(id: id, task: task, limit: limit.intValue)
        lock.unlock(); task.resume()
    }
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        lock.lock(); let transfer = transfers[dataTask.taskIdentifier]; lock.unlock()
        guard let transfer, let response = response as? HTTPURLResponse, let url = response.url, validURL(url) else { completionHandler(.cancel); return }
        let headers = response.allHeaderFields.map { [String(describing: $0.key), String(describing: $0.value)] }
        event?(transfer.id, ["type": "headers", "status": response.statusCode, "url": url.absoluteString, "headers": headers])
        completionHandler(.allow)
    }
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        lock.lock()
        guard var transfer = transfers[dataTask.taskIdentifier] else { lock.unlock(); return }
        guard data.count <= transfer.limit - transfer.received else {
            transfers.removeValue(forKey: dataTask.taskIdentifier); lock.unlock(); dataTask.cancel()
            event?(transfer.id, ["type": "failed", "message": "HTTP response byte limit exceeded"]); return
        }
        transfer.received += data.count; transfers[dataTask.taskIdentifier] = transfer; lock.unlock()
        for offset in stride(from: 0, to: data.count, by: 64 * 1024) {
            event?(transfer.id, ["type": "chunk", "bytes": Array(data[offset..<min(offset + 64 * 1024, data.count)])])
        }
    }
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        lock.lock()
        guard var transfer = transfers[task.taskIdentifier], transfer.redirects < 5,
              let url = request.url, validURL(url), !(response.url?.scheme == "https" && url.scheme == "http") else {
            lock.unlock(); completionHandler(nil); return
        }
        transfer.redirects += 1; transfers[task.taskIdentifier] = transfer; lock.unlock()
        var redirected = request
        if response.url?.host != request.url?.host || response.url?.port != request.url?.port || response.url?.scheme != request.url?.scheme {
            redirected.setValue(nil, forHTTPHeaderField: "Authorization")
            redirected.setValue(nil, forHTTPHeaderField: "Cookie")
        }
        completionHandler(redirected)
    }
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        lock.lock(); let transfer = transfers.removeValue(forKey: task.taskIdentifier); lock.unlock()
        guard let transfer else { return }
        if let error { event?(transfer.id, ["type": "failed", "message": String(error.localizedDescription.prefix(1024))]) }
        else { event?(transfer.id, ["type": "complete"]) }
    }
    func dispose() {
        lock.lock(); closed = true; let tasks = transfers.values.map(\.task); transfers.removeAll(); lock.unlock()
        tasks.forEach { $0.cancel() }; session.invalidateAndCancel(); event = nil
    }
}

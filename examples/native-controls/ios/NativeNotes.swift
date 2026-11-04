import UIKit
import Foundation

private enum NotesStorageError: Error { case invalid(String) }

private final class NoteDocuments {
    private static let maximumBytes = 512 * 1024
    private let url: URL
    private var observed: Data?

    init() throws {
        let manager = FileManager.default
        guard let support = manager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            throw NotesStorageError.invalid("Application Support is unavailable")
        }
        let directory = support.appendingPathComponent("JadixNotes", isDirectory: true)
        try manager.createDirectory(at: directory, withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.complete.rawValue])
        url = directory.appendingPathComponent("notebook-v1.json")
    }

    private func validate(checkSize: Bool) throws -> Bool {
        do {
            let properties = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
            guard properties.isRegularFile == true, properties.isSymbolicLink != true else {
                throw NotesStorageError.invalid("Invalid notebook file")
            }
            if checkSize {
                guard let size = properties.fileSize, size <= Self.maximumBytes else {
                    throw NotesStorageError.invalid("Notebook exceeds 512 KiB")
                }
            }
            return true
        } catch {
            let failure = error as NSError
            if failure.domain == NSCocoaErrorDomain && failure.code == NSFileReadNoSuchFileError { return false }
            throw error
        }
    }

    private func bytes() throws -> Data? {
        guard try validate(checkSize: true) else { return nil }
        let data = try Data(contentsOf: url)
        guard data.count <= Self.maximumBytes else { throw NotesStorageError.invalid("Notebook exceeds 512 KiB") }
        return data
    }

    func read() throws -> String? {
        precondition(Thread.isMainThread)
        observed = try bytes()
        guard let data = observed else { return nil }
        guard let source = String(data: data, encoding: .utf8) else { throw NotesStorageError.invalid("Invalid notebook UTF-8") }
        return source
    }

    func write(_ source: String, replaceConfirmed: Bool) throws {
        precondition(Thread.isMainThread)
        let data = Data(source.utf8)
        guard data.count <= Self.maximumBytes else { throw NotesStorageError.invalid("Notebook exceeds 512 KiB") }
        _ = try validate(checkSize: false)
        if !replaceConfirmed {
            let current = try bytes()
            guard current == observed else {
                throw NotesStorageError.invalid("Notebook changed in another window; export and reopen or confirm Import")
            }
        }
        try data.write(to: url, options: [.atomic, .completeFileProtection])
        guard try bytes() == data else { throw NotesStorageError.invalid("Notebook write verification failed") }
        observed = data
    }
}

final class NativeNotesController: NativeController {
    private var documents: NoteDocuments?
    private var persisted: String?
    private var blocked = false

    private func object(_ source: String) throws -> [String: Any] {
        guard let result = try JSONSerialization.jsonObject(with: Data(source.utf8)) as? [String: Any] else {
            throw NotesStorageError.invalid("Invalid notebook state")
        }
        return result
    }

    private func archive(_ state: [String: Any]) throws -> String {
        guard let notebook = state["archive"] as? [String: Any] else { throw NotesStorageError.invalid("Missing notebook") }
        return String(decoding: try JSONSerialization.data(withJSONObject: notebook, options: [.sortedKeys]), as: UTF8.self)
    }

    private func report(_ error: Error) {
        let message = String(describing: error)
        try? native.storageResult(message + "; export a backup, retry Save, or confirm a valid Import")
        NSLog("Jadix notes: %@", message)
    }

    override func didMount() {
        do {
            documents = try NoteDocuments()
            if let source = try documents?.read() { try native.restore(source) }
        } catch { blocked = true; report(error) }
        do { persisted = try archive(object(native.state())) } catch { report(error) }
        native.onStateChange = { [weak self] source in self?.persist(source) }
    }

    private func persist(_ source: String) {
        do {
            let state = try object(source)
            let document = try archive(state)
            guard document != persisted || state["storage_pending"] as? Bool == true else { return }
            let replace = state["replacement_confirmed"] as? Bool == true
            if blocked && !replace { throw NotesStorageError.invalid("Stored notebook unavailable; confirm Import to replace it") }
            if documents == nil { documents = try NoteDocuments() }
            guard let documents = documents else { throw NotesStorageError.invalid("Notebook storage unavailable") }
            try documents.write(document, replaceConfirmed: replace)
            persisted = document
            blocked = false
            try native.storageResult(nil)
        } catch { report(error) }
    }
}

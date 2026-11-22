import AppKit
import Darwin

@MainActor private final class JadixTerminalText: NSTextView {
    var send: ((String) -> Void)?
    var applicationCursor = false
    var bracketedPaste = false
    override func keyDown(with event: NSEvent) {
        if event.modifierFlags.contains(.command) { super.keyDown(with: event); return }
        let keys: [UInt16: String] = [123: "left", 124: "right", 125: "down", 126: "up", 115: "home", 119: "end",
            36: "return", 76: "return", 48: "tab", 51: "backspace", 53: "escape", 117: "delete", 114: "insert", 116: "page_up", 121: "page_down",
            122: "f1", 120: "f2", 99: "f3", 118: "f4", 96: "f5", 97: "f6", 98: "f7", 100: "f8", 101: "f9", 109: "f10", 103: "f11", 111: "f12"]
        if let key = keys[event.keyCode], let sequence = JadixTerminalInput.special(key, shift: event.modifierFlags.contains(.shift),
                alt: event.modifierFlags.contains(.option), control: event.modifierFlags.contains(.control), applicationCursor: applicationCursor) {
            send?(sequence); return
        }
        if event.modifierFlags.contains(.control), let char = event.charactersIgnoringModifiers?.unicodeScalars.first, char.value < 128 {
            send?(String(UnicodeScalar(char.value & 0x1f)!)); return
        }
        if event.modifierFlags.contains(.option), !event.modifierFlags.contains(.command), let chars = event.charactersIgnoringModifiers {
            send?("\u{1b}" + chars); return
        }
        super.keyDown(with: event)
    }
    override func paste(_ sender: Any?) {
        guard let value = NSPasteboard.general.string(forType: .string), value.utf8.count <= 64 * 1024 - 12 else { return }
        send?(bracketedPaste ? "\u{1b}[200~" + value + "\u{1b}[201~" : value)
    }
}

@MainActor final class JadixTerminalView: NSView, NSTextViewDelegate {
    private let textView = JadixTerminalText()
    private let scroll = NSScrollView()
    private var pid: Int32 = 0
    private var master: Int32 = -1
    private var reader: DispatchSourceRead?
    private var writer: DispatchSourceWrite?
    private var pendingInput = Data()
    private var screen: UInt64 = 0
    private var generation = UUID()
    private var columns: UInt16 = 80
    private var rows: UInt16 = 24
    private var requestedSize: (UInt16, UInt16)?
    private var inputRevision: UInt64?
    var emit: ((String, Any) -> Void)?
    override init(frame: NSRect) {
        super.init(frame: frame)
        scroll.hasVerticalScroller = true; scroll.documentView = textView; addSubview(scroll)
        textView.isEditable = true; textView.isRichText = false; textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.font = .monospacedSystemFont(ofSize: 12, weight: .regular); textView.delegate = self
        textView.textContainer?.widthTracksTextView = true; textView.isVerticallyResizable = true
        textView.send = { [weak self] bytes in self?.send(bytes) }
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func layout() {
        super.layout(); scroll.frame = bounds
        let width = ("M" as NSString).size(withAttributes: [.font: textView.font!]).width
        let height = (textView.font?.ascender ?? 12) - (textView.font?.descender ?? -3) + (textView.font?.leading ?? 0)
        let size = requestedSize ?? (UInt16(max(1, min(512, Int((bounds.width - 12) / max(1, width))))), UInt16(max(1, min(256, Int(bounds.height / max(1, height))))))
        resize(size.0, size.1)
    }
    func update(_ value: [String: Any]) {
        if let c = value["columns"] as? Int, let r = value["rows"] as? Int, (1...512).contains(c), (1...256).contains(r) { requestedSize = (UInt16(c), UInt16(r)) }
        if screen == 0 {
            if let size = requestedSize { columns = size.0; rows = size.1 }
            let history = min(max(0, value["scrollback"] as? Int ?? 10_000), 1_000_000 / Int(columns) - Int(rows))
            screen = jadix_terminal_screen_new(columns, rows, history)
            guard screen != 0 else { emit?("error", "Invalid terminal dimensions"); return }
            start(value["command"] as? String ?? "/bin/zsh", arguments: value["arguments"] as? [String] ?? [])
        }
        if let revision = (value["input_revision"] as? NSNumber)?.uint64Value, revision != inputRevision, let bytes = value["input"] as? String {
            inputRevision = revision; send(bytes)
        }
        needsLayout = true
    }
    private func start(_ command: String, arguments: [String]) {
        guard command.hasPrefix("/"), !command.utf8.contains(0), arguments.count <= 128,
              arguments.allSatisfy({ $0.utf8.count <= 32 * 1024 && !$0.utf8.contains(0) }) else { emit?("error", "Invalid terminal command"); return }
        let strings = (["/usr/bin/env", "TERM=xterm-256color", command] + arguments).map { strdup($0) }
        defer { strings.forEach { free($0) } }
        guard strings.allSatisfy({ $0 != nil }) else { emit?("error", "Terminal allocation failed"); return }
        var argv = strings.map { $0.map { UnsafePointer($0) } }; argv.append(nil)
        var fd: Int32 = -1
        pid = argv.withUnsafeBufferPointer { JadixTerminalSpawn($0.baseAddress, columns, rows, &fd) }
        guard pid > 0, fd >= 0 else { pid = 0; emit?("error", "PTY creation failed"); return }
        let token = generation, child = pid
        master = fd
        let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: .main)
        source.setEventHandler { [weak self] in
            guard let self, self.generation == token, self.master == fd else { return }
            var buffer = [UInt8](repeating: 0, count: 64 * 1024)
            let count = Darwin.read(fd, &buffer, buffer.count)
            if count > 0 { self.consume(Data(buffer.prefix(count))) }
            else if count == 0 || (errno != EAGAIN && errno != EINTR) { self.reader?.cancel(); self.reader = nil }
        }
        reader = source; source.resume()
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let code = JadixTerminalWait(child)
            DispatchQueue.main.async { [weak self] in guard let self, self.generation == token else { return }; self.emit?("exit", ["code": code]) }
        }
    }
    private func resize(_ columns: UInt16, _ rows: UInt16) {
        guard screen != 0, self.columns != columns || self.rows != rows,
              jadix_terminal_screen_resize(screen, columns, rows) else { return }
        self.columns = columns; self.rows = rows
        if master >= 0 { _ = JadixTerminalResize(master, columns, rows) }
        emit?("resize", ["columns": columns, "rows": rows])
    }
    private func send(_ bytes: String) {
        guard master >= 0, bytes.utf8.count <= 64 * 1024 - pendingInput.count else { return }
        pendingInput.append(contentsOf: bytes.utf8); flushInput(); emit?("input", ["bytes": bytes])
    }
    private func flushInput() {
        guard master >= 0 else { return }
        while !pendingInput.isEmpty {
            let count = pendingInput.withUnsafeBytes { Darwin.write(master, $0.baseAddress, $0.count) }
            if count > 0 { pendingInput.removeFirst(count); continue }
            if count < 0 && errno == EINTR { continue }
            if count < 0 && errno == EAGAIN {
                if writer == nil {
                    let source = DispatchSource.makeWriteSource(fileDescriptor: master, queue: .main)
                    source.setEventHandler { [weak self] in self?.flushInput() }; writer = source; source.resume()
                }
                return
            }
            pendingInput.removeAll(); break
        }
        writer?.cancel(); writer = nil
    }
    private func consume(_ data: Data) {
        guard screen != 0 else { return }
        let result = data.withUnsafeBytes { bytes in jadix_terminal_screen_feed(screen, bytes.bindMemory(to: UInt8.self).baseAddress, data.count) }
        guard let result else { return }; defer { jadix_native_string_free(result) }
        guard let frame = try? JSONSerialization.jsonObject(with: Data(String(cString: result).utf8)) as? [String: Any], let runs = frame["runs"] as? [[String: Any]] else { return }
        let display = NSMutableAttributedString(string: "")
        func color(_ rgb: NSNumber?) -> NSColor? { guard let n = rgb?.uint32Value else { return nil }; return NSColor(srgbRed: CGFloat((n >> 16) & 255) / 255, green: CGFloat((n >> 8) & 255) / 255, blue: CGFloat(n & 255) / 255, alpha: 1) }
        for run in runs {
            guard let text = run["text"] as? String, let style = run["style"] as? [String: Any] else { continue }
            var font = NSFont.monospacedSystemFont(ofSize: 12, weight: style["bold"] as? Bool == true ? .bold : .regular)
            if style["italic"] as? Bool == true { font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask) }
            var fg = color(style["foreground"] as? NSNumber) ?? .textColor, bg = color(style["background"] as? NSNumber) ?? .textBackgroundColor
            if style["inverse"] as? Bool == true { swap(&fg, &bg) }
            display.append(NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: fg, .backgroundColor: bg, .underlineStyle: style["underline"] as? Bool == true ? NSUnderlineStyle.single.rawValue : 0]))
        }
        let followsOutput = scroll.contentView.bounds.maxY >= textView.bounds.maxY - 24
        textView.textStorage?.setAttributedString(display)
        if let cursor = frame["cursor_utf16"] as? Int { textView.setSelectedRange(NSRange(location: min(cursor, display.length), length: 0)) }
        textView.insertionPointColor = frame["cursor_visible"] as? Bool == false ? .clear : .textColor
        if followsOutput { textView.scrollToEndOfDocument(nil) }
        textView.applicationCursor = frame["application_cursor"] as? Bool ?? false; textView.bracketedPaste = frame["bracketed_paste"] as? Bool ?? false
        for reply in frame["replies"] as? [String] ?? [] { send(reply) }
        emit?("output", ["data": Array(data)])
    }
    func textView(_ textView: NSTextView, shouldChangeTextIn affectedCharRange: NSRange, replacementString: String?) -> Bool {
        if let replacementString { send(replacementString) }; return false
    }
    func dispose() {
        generation = UUID(); let child = pid; pid = 0
        reader?.cancel(); reader = nil; writer?.cancel(); writer = nil
        if master >= 0 { Darwin.close(master); master = -1 }; pendingInput.removeAll()
        if child > 0 { DispatchQueue.global(qos: .utility).async { JadixTerminalStop(child) } }
        if screen != 0 { jadix_terminal_screen_free(screen); screen = 0 }
    }
}

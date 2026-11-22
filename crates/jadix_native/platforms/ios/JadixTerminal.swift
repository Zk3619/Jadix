import UIKit

@MainActor private final class JadixTerminalText: UITextView {
    var send: ((String) -> Void)?
    var applicationCursor = false, bracketedPaste = false
    var compositionEnded: (() -> Void)?
    private var composing = false
    var acceptsCompositionEdit: Bool { composing || markedTextRange != nil }
    override func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        let committed = markedTextRange.map { text(in: $0) ?? "" } ?? ""
        let shouldSend = composing && markedText == nil
        composing = markedText != nil
        super.setMarkedText(markedText, selectedRange: selectedRange)
        if shouldSend, !committed.isEmpty { send?(committed) }
        if markedText == nil { compositionEnded?() }
    }
    override func unmarkText() {
        let committed = markedTextRange.map { text(in: $0) ?? "" } ?? ""
        let shouldSend = composing; composing = false; super.unmarkText()
        if shouldSend, !committed.isEmpty { send?(committed) }
        compositionEnded?()
    }
    override func insertText(_ value: String) {
        // UIKit supplies the committed replacement here. Never echo transport
        // input into the output buffer, including when committing an IME range.
        composing = false; super.unmarkText()
        send?(value.replacingOccurrences(of: "\n", with: "\r")); compositionEnded?()
    }
    override func deleteBackward() {
        if markedTextRange != nil { super.deleteBackward() }
        else { send?("\u{7f}") }
    }
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard markedTextRange == nil else { super.pressesBegan(presses, with: event); return }
        var handled = Set<UIPress>()
        for press in presses {
            guard let key = press.key, !key.modifierFlags.contains(.command) else { continue }
            let special: [UIKeyboardHIDUsage: String] = [
                .keyboardUpArrow: "up", .keyboardDownArrow: "down", .keyboardRightArrow: "right", .keyboardLeftArrow: "left",
                .keyboardHome: "home", .keyboardEnd: "end", .keyboardPageUp: "page_up", .keyboardPageDown: "page_down",
                .keyboardInsert: "insert", .keyboardDeleteForward: "delete", .keyboardDeleteOrBackspace: "backspace",
                .keyboardReturnOrEnter: "return", .keyboardEscape: "escape", .keyboardTab: "tab",
                .keyboardF1: "f1", .keyboardF2: "f2", .keyboardF3: "f3", .keyboardF4: "f4", .keyboardF5: "f5", .keyboardF6: "f6",
                .keyboardF7: "f7", .keyboardF8: "f8", .keyboardF9: "f9", .keyboardF10: "f10", .keyboardF11: "f11", .keyboardF12: "f12"]
            var output = special[key.keyCode].flatMap { JadixTerminalInput.special($0, shift: key.modifierFlags.contains(.shift), alt: key.modifierFlags.contains(.alternate), control: key.modifierFlags.contains(.control), applicationCursor: applicationCursor) }
            if output == nil, key.modifierFlags.contains(.control), let scalar = key.charactersIgnoringModifiers.uppercased().unicodeScalars.first, (64...95).contains(scalar.value) {
                output = String(UnicodeScalar(scalar.value - 64)!)
            } else if output == nil, key.modifierFlags.contains(.alternate), !key.characters.isEmpty { output = "\u{1b}" + key.characters }
            if let output { send?(output); handled.insert(press) }
        }
        let remaining = presses.subtracting(handled)
        if !remaining.isEmpty { super.pressesBegan(remaining, with: event) }
    }
    override func paste(_ sender: Any?) {
        guard let text = UIPasteboard.general.string, text.utf8.count <= 64 * 1024 - 12 else { return }
        send?(bracketedPaste ? "\u{1b}[200~" + text + "\u{1b}[201~" : text)
    }
}

/// An ANSI screen backed by the same bounded Rust parser as the desktop host.
/// iOS applications supply transport bytes; spawning arbitrary processes is not
/// an iOS application capability.
@MainActor final class JadixTerminalView: UIView, UITextViewDelegate {
    private let text = JadixTerminalText()
    private var screen: UInt64 = 0
    private var revision: UInt64?
    private var columns: UInt16 = 80, rows: UInt16 = 24
    private var history = 10_000
    private var pendingDisplay: NSAttributedString?
    private var pendingCursor = 0
    var emit: ((String, Any) -> Void)?
    override init(frame: CGRect) {
        super.init(frame: frame); addSubview(text); text.delegate = self
        text.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        text.autocorrectionType = .no; text.autocapitalizationType = .none; text.smartQuotesType = .no; text.smartDashesType = .no
        text.send = { [weak self] in self?.send($0) }; text.compositionEnded = { [weak self] in self?.renderPending() }; text.accessibilityLabel = "Terminal"
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    override func layoutSubviews() { super.layoutSubviews(); text.frame = bounds }
    func update(_ value: [String: Any]) {
        let c = value["columns"] as? Int ?? 80, r = value["rows"] as? Int ?? 24
        guard (1...512).contains(c), (1...256).contains(r) else { emit?("error", "Invalid terminal dimensions"); return }
        let nextHistory = max(0, min(value["scrollback"] as? Int ?? 10_000, 1_000_000 / c - r))
        if screen == 0 {
            columns = UInt16(c); rows = UInt16(r); history = nextHistory; screen = jadix_terminal_screen_new(columns, rows, history)
            guard screen != 0 else { emit?("error", "Terminal screen allocation failed"); return }
            display(jadix_terminal_screen_frame(screen))
        }
        else if columns != UInt16(c) || rows != UInt16(r) || history != nextHistory {
            if jadix_terminal_screen_configure(screen, UInt16(c), UInt16(r), nextHistory) {
                columns = UInt16(c); rows = UInt16(r); history = nextHistory
                display(jadix_terminal_screen_frame(screen)); emit?("resize", ["columns": c, "rows": r])
            } else { emit?("error", "Terminal reconfiguration failed") }
        }
        if value["command"] != nil { emit?("error", "iOS terminal requires an application-owned transport") }
        if let next = (value["input_revision"] as? NSNumber)?.uint64Value, next != revision, let bytes = value["input"] as? String, bytes.utf8.count <= 65536 {
            revision = next; consume(Data(bytes.utf8))
        }
    }
    private func send(_ value: String) { if value.utf8.count <= 65536 { emit?("input", ["bytes": value]) } }
    private func consume(_ data: Data) {
        guard screen != 0 else { return }
        let pointer = data.withUnsafeBytes { jadix_terminal_screen_feed(screen, $0.bindMemory(to: UInt8.self).baseAddress, data.count) }
        display(pointer); emit?("output", ["data": Array(data)])
    }
    private func display(_ pointer: UnsafeMutablePointer<CChar>?) {
        guard let pointer else { return }; defer { jadix_native_string_free(pointer) }
        guard let frame = try? JSONSerialization.jsonObject(with: Data(String(cString: pointer).utf8)) as? [String: Any] else { return }
        let display = NSMutableAttributedString(string: "")
        func color(_ number: NSNumber?) -> UIColor? {
            guard let n = number?.uint32Value else { return nil }
            return UIColor(red: CGFloat((n >> 16) & 255) / 255, green: CGFloat((n >> 8) & 255) / 255, blue: CGFloat(n & 255) / 255, alpha: 1)
        }
        for run in frame["runs"] as? [[String: Any]] ?? [] {
            guard let value = run["text"] as? String, let style = run["style"] as? [String: Any] else { continue }
            var font = UIFont.monospacedSystemFont(ofSize: 12, weight: style["bold"] as? Bool == true ? .bold : .regular)
            if style["italic"] as? Bool == true, let descriptor = font.fontDescriptor.withSymbolicTraits(.traitItalic) { font = UIFont(descriptor: descriptor, size: 12) }
            var fg = color(style["foreground"] as? NSNumber) ?? .label, bg = color(style["background"] as? NSNumber) ?? .systemBackground
            if style["inverse"] as? Bool == true { swap(&fg, &bg) }
            display.append(NSAttributedString(string: value, attributes: [.font: font, .foregroundColor: fg, .backgroundColor: bg, .underlineStyle: style["underline"] as? Bool == true ? 1 : 0]))
        }
        pendingCursor = max(0, min(frame["cursor_utf16"] as? Int ?? display.length, display.length))
        text.tintColor = frame["cursor_visible"] as? Bool == false ? .clear : .label
        pendingDisplay = display; renderPending()
        text.applicationCursor = frame["application_cursor"] as? Bool ?? false; text.bracketedPaste = frame["bracketed_paste"] as? Bool ?? false
        for reply in frame["replies"] as? [String] ?? [] { send(reply) }
    }
    private func renderPending() {
        guard text.markedTextRange == nil, let display = pendingDisplay else { return }
        let selection = text.selectedRange, follows = text.contentOffset.y + text.bounds.height >= text.contentSize.height - 24
        text.attributedText = display
        // Preserve user selection after long-press/touch end and output updates.
        text.selectedRange = JadixRichText.selection(selection.length == 0 ? NSRange(location: pendingCursor, length: 0) : selection, in: display.string)
        if follows, selection.length == 0 { text.scrollRangeToVisible(text.selectedRange) }
    }
    func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText: String) -> Bool {
        text.acceptsCompositionEdit
    }
    func dispose() { emit = nil; pendingDisplay = nil; text.compositionEnded = nil; text.send = nil; text.delegate = nil; if screen != 0 { jadix_terminal_screen_free(screen); screen = 0 } }
    deinit { if screen != 0 { jadix_terminal_screen_free(screen) } }
}

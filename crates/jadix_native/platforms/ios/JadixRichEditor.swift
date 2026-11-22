import UIKit

// Attachment ownership belongs to the editor; parsing and dimensions come from
// jadix_document, while decoding/authorization use the root's ordinary loader.
private final class JadixRichAttachment: NSTextAttachment {
    let resource: JadixRichText.Image
    init(_ resource: JadixRichText.Image) {
        self.resource = resource
        super.init(data: nil, ofType: nil)
        bounds = CGRect(origin: .zero, size: JadixRichText.imageSize(resource))
    }
    required init?(coder: NSCoder) { nil }
}

@MainActor final class JadixRichEditor: UITextView, UITextViewDelegate {
    var emit: ((String, Any) -> Void)?
    weak var remoteImages: AppleRemoteImages?
    private var revision: UInt64 = 0
    private var appliedHTML: String?
    private var pending: [String: Any]?
    private var applying = false
    private var imageGeneration = UUID()
    private var imageQueue: [JadixRichAttachment] = []
    private var transfers: [UUID: (url: URL, token: UUID)] = [:]
    private var loading: Set<UUID> = []
    private var imageBytes = 0
    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        delegate = self; isSelectable = true; allowsEditingTextAttributes = true
    }
    required init?(coder: NSCoder) { nil }
    func update(_ value: [String: Any]) {
        let next = (value["revision"] as? NSNumber)?.uint64Value ?? 0
        guard next >= revision, next >= ((pending?["revision"] as? NSNumber)?.uint64Value ?? 0) else { return }
        if markedTextRange != nil { pending = value; return }
        guard let html = value["html"] as? String else { return }
        applying = true; defer { applying = false }
        if html != appliedHTML {
            guard let plan = JadixRichText.plan(html), let attributed = try? NSMutableAttributedString(
                data: Data(plan.html.utf8), options: [.documentType: NSAttributedString.DocumentType.html,
                    .characterEncoding: String.Encoding.utf8.rawValue], documentAttributes: nil)
                else { emit?("error", "Invalid rich editor HTML"); return }
            var replacements: [(NSRange, JadixRichAttachment)] = []
            attributed.enumerateAttribute(.link, in: NSRange(location: 0, length: attributed.length)) { value, range, _ in
                let link = (value as? URL)?.absoluteString ?? value as? String ?? ""
                guard link.hasPrefix("jadix-attachment:"), let index = Int(link.dropFirst("jadix-attachment:".count)),
                      plan.images.indices.contains(index), range.length == 1,
                      (attributed.string as NSString).substring(with: range) == "\u{fffc}" else { return }
                replacements.append((range, JadixRichAttachment(plan.images[index])))
            }
            for (range, attachment) in replacements.reversed() {
                let attributes = attributed.attributes(at: range.location, effectiveRange: nil)
                let replacement = NSMutableAttributedString(attachment: attachment)
                replacement.addAttributes(attributes, range: NSRange(location: 0, length: 1))
                replacement.removeAttribute(.link, range: NSRange(location: 0, length: 1))
                attributed.replaceCharacters(in: range, with: replacement)
            }
            cancelImages()
            let previous = selectedRange
            undoManager?.removeAllActions(); attributedText = attributed
            selectedRange = JadixRichText.selection(previous, in: text)
            appliedHTML = html
            imageQueue = replacements.map { $0.1 }; loadImages()
        }
        revision = next; isEditable = value["editable"] as? Bool == true
        if let range = value["selection"] as? [Int], range.count == 2 { selectedRange = JadixRichText.selection(range, in: text) }
    }
    override func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        super.setMarkedText(markedText, selectedRange: selectedRange)
        if markedText == nil { committed() }
    }
    override func unmarkText() { super.unmarkText(); committed() }
    override func insertText(_ text: String) { super.insertText(text); if markedTextRange == nil { committed() } }
    private func committed() {
        guard !applying, markedTextRange == nil else { return }
        textViewDidChange(self)
        if let value = pending { pending = nil; update(value) }
    }
    func textViewDidChange(_ textView: UITextView) {
        guard !applying, markedTextRange == nil else { return }
        guard let html = exportedHTML() else { emit?("error", "Rich editor output exceeds budget"); return }
        guard html != appliedHTML else { return }
        appliedHTML = html; emit?("change", ["html": html, "revision": revision])
    }
    func textViewDidChangeSelection(_ textView: UITextView) {
        guard !applying, markedTextRange == nil else { return }
        emit?("selection", ["range": [selectedRange.location, NSMaxRange(selectedRange)], "revision": revision])
    }
    func textViewDidBeginEditing(_ textView: UITextView) { emit?("focus", ["focused": true, "revision": revision]) }
    func textViewDidEndEditing(_ textView: UITextView) { committed(); emit?("focus", ["focused": false, "revision": revision]) }
    private func exportedHTML() -> String? {
        guard attributedText.length <= 1_048_576 else { return nil }
        let copy = NSMutableAttributedString(attributedString: attributedText)
        var replacements: [(NSRange, String, String)] = []
        var invalid = false
        copy.enumerateAttribute(.attachment, in: NSRange(location: 0, length: copy.length)) { value, range, _ in
            guard let attachment = value as? NSTextAttachment else { return }
            guard replacements.count < 128, range.length == 1 else { invalid = true; return }
            let html: String
            if let attachment = attachment as? JadixRichAttachment { html = attachment.resource.html }
            else {
                // Native paste can contain an image with no source URL. Admit
                // a bounded inline PNG through the same shared resource parser.
                guard let image = attachment.image, image.size.width.isFinite, image.size.height.isFinite,
                      image.size.width > 0, image.size.height > 0,
                      image.size.width * image.size.height * image.scale * image.scale <= 1_048_576,
                      let png = image.pngData(), png.count <= 700_000,
                      let resource = JadixRichText.plan("<img src=\"data:image/png;base64,\(png.base64EncodedString())\">")?.images.first
                      else { invalid = true; return }
                html = resource.html
            }
            var marker: String
            repeat { marker = "JADIXIMAGE" + UUID().uuidString.replacingOccurrences(of: "-", with: "") }
                while copy.string.contains(marker)
            replacements.append((range, marker, html))
        }
        guard !invalid else { return nil }
        for (range, marker, _) in replacements.reversed() {
            copy.replaceCharacters(in: range, with: NSAttributedString(string: marker))
        }
        guard var html = JadixRichText.html(copy) else { return nil }
        for (_, marker, image) in replacements { html = html.replacingOccurrences(of: marker, with: image) }
        return html.utf8.count <= 1_048_576 ? html : nil
    }
    private func loadImages() {
        guard let loader = remoteImages else { return }
        while loading.count < 4, !imageQueue.isEmpty {
            let attachment = imageQueue.removeFirst(), request = UUID(), generation = imageGeneration
            loading.insert(request)
            var imageRequest = AppleImageRequest(source: attachment.resource.source, pixelWidth: 512, pixelHeight: 512, fit: .contain)
            imageRequest.allowsAnimation = false
            let transfer = loader.decode(imageRequest) { [weak self, weak attachment] decoded in
                guard let self, self.imageGeneration == generation else { return }
                self.loading.remove(request); self.transfers.removeValue(forKey: request)
                if let attachment, let decoded, let frame = decoded.frames.first {
                    let cost = frame.bytesPerRow * frame.height
                    if cost <= 16 * 1024 * 1024 - self.imageBytes {
                        self.imageBytes += cost
                        attachment.image = UIImage(cgImage: frame)
                        attachment.bounds.size = JadixRichText.imageSize(attachment.resource, natural: decoded.originalSize)
                        let range = NSRange(location: 0, length: self.textStorage.length)
                        self.layoutManager.invalidateLayout(forCharacterRange: range, actualCharacterRange: nil)
                        self.layoutManager.invalidateDisplay(forCharacterRange: range)
                        self.invalidateIntrinsicContentSize(); self.setNeedsLayout()
                    }
                }
                self.loadImages()
            }
            if loading.contains(request), let transfer { transfers[request] = transfer }
        }
    }
    private func cancelImages() {
        imageGeneration = UUID()
        for transfer in transfers.values { remoteImages?.cancel(transfer.url, token: transfer.token) }
        transfers.removeAll(); loading.removeAll(); imageQueue.removeAll(); imageBytes = 0
    }
    func dispose() { emit = nil; delegate = nil; pending = nil; cancelImages(); undoManager?.removeAllActions(); resignFirstResponder() }
}

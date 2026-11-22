import UIKit

/// Stable portable IDs are independent of UIKit's item positions and tags.
@MainActor final class JadixTabBar: UITabBar, UITabBarDelegate {
    private var ids: [String] = []
    private var selectedID: String?
    private var select: ((String) -> Void)?
    private var enabled = true
    private var ownedItems: [UITabBarItem] = []
    private var lastLayoutSize: CGSize?
    private var itemGeneration = UUID()
    private var layoutQueued = false
    override func didMoveToWindow() {
        super.didMoveToWindow(); lastLayoutSize = nil; setNeedsLayout()
    }
    override func layoutSubviews() {
        super.layoutSubviews()
        guard window != nil, bounds.width >= 1, bounds.height >= 1,
              bounds.size != lastLayoutSize, !layoutQueued else { return }
        lastLayoutSize = bounds.size; layoutQueued = true
        let generation = itemGeneration
        // UIKit can skip label layout when items are replaced inside its own
        // layout pass. Reattach on the next turn for both width and height changes.
        setItems(nil, animated: false)
        DispatchQueue.main.async { [weak self] in
            guard let self, self.itemGeneration == generation else { return }
            self.layoutQueued = false
            guard self.window != nil else { self.lastLayoutSize = nil; return }
            self.restoreItems(); self.setNeedsLayout()
        }
    }
    private func restoreItems() {
        setItems(ownedItems, animated: false)
        selectedItem = selectedID.flatMap { ids.firstIndex(of: $0) }.flatMap { ownedItems.indices.contains($0) ? ownedItems[$0] : nil }
    }
    override init(frame: CGRect) { super.init(frame: frame); delegate = self }
    required init?(coder: NSCoder) { super.init(coder: coder); delegate = self }

    func update(_ value: [String: Any], select: @escaping (String) -> Void) {
        self.select = select; enabled = value["enabled"] as? Bool == true
        isHidden = value["hidden"] as? Bool == true
        isUserInteractionEnabled = enabled
        let entries = value["items"] as? [[String: Any]] ?? []
        ids = entries.map { $0["id"] as? String ?? "" }
        selectedID = value["selected"] as? String
        itemGeneration = UUID(); layoutQueued = false; lastLayoutSize = nil
        ownedItems = entries.enumerated().map { index, entry in
            let image = (entry["symbol"] as? String).flatMap { UIImage(systemName: $0) }
            let item = UITabBarItem(title: entry["label"] as? String, image: image, tag: index)
            item.isEnabled = enabled && entry["enabled"] as? Bool == true
            item.badgeValue = entry["badge"] as? String
            item.accessibilityLabel = entry["accessibility_label"] as? String ?? entry["label"] as? String
            return item
        }
        restoreItems(); setNeedsLayout()
        invalidateIntrinsicContentSize()
    }
    func tabBar(_ tabBar: UITabBar, didSelect item: UITabBarItem) {
        guard enabled, !isHidden, window != nil, item.isEnabled, ids.indices.contains(item.tag) else { return }
        let id = ids[item.tag]
        // Selection is controlled by the shared model, including a rejected event.
        selectedItem = selectedID.flatMap { ids.firstIndex(of: $0) }.flatMap { items?[$0] }
        select?(id)
    }
    override var canBecomeFirstResponder: Bool { enabled && !isHidden }
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard enabled, !isHidden, let key = presses.first?.key,
              key.modifierFlags.intersection([.command, .control, .alternate]).isEmpty else {
            super.pressesBegan(presses, with: event); return
        }
        let rtl = effectiveUserInterfaceLayoutDirection == .rightToLeft
        let step: Int
        switch key.keyCode {
        case .keyboardLeftArrow: step = rtl ? 1 : -1
        case .keyboardRightArrow: step = rtl ? -1 : 1
        default: super.pressesBegan(presses, with: event); return
        }
        guard let items, !items.isEmpty else { return }
        let current = selectedID.flatMap { ids.firstIndex(of: $0) } ?? (step > 0 ? -1 : 0)
        for offset in 1...items.count {
            let index = (current + step * offset + items.count * 2) % items.count
            if items[index].isEnabled { tabBar(self, didSelect: items[index]); break }
        }
    }
    func retire() { itemGeneration = UUID(); ownedItems.removeAll(); lastLayoutSize = nil; layoutQueued = false; select = nil; ids.removeAll(); selectedID = nil; setItems([], animated: false) }
}

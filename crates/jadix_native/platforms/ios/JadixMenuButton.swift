import UIKit

/// The portable menu specification owns IDs and selection semantics; UIKit owns
/// menu placement, scrolling, touch navigation and the accessibility presentation.
@MainActor final class JadixMenuButton: JadixButton {
    private var revision = UUID().uuidString
    private var commands: [UIKeyCommand] = []
    private var selections: [String: () -> Void] = [:]

    func updateMenu(_ items: [[String: Any]], select: @escaping (String) -> Void) {
        retire()
        let token = revision
        func build(_ items: [[String: Any]], enabled: Bool) -> [UIMenuElement] {
            var groups: [UIMenuElement] = [], group: [UIMenuElement] = []
            func flush() {
                if !group.isEmpty { groups.append(UIMenu(options: .displayInline, children: group)); group.removeAll() }
            }
            for item in items {
                if item["kind"] as? String == "separator" { flush(); continue }
                let label = item["label"] as? String ?? ""
                let available = enabled && item["enabled"] as? Bool != false
                let source = item["symbol"] as? String ?? (item["icon"] as? [String: Any])?["source"] as? String
                let image = source.flatMap { UIImage(systemName: $0.replacingOccurrences(of: "sf:", with: "")) }
                let nested = item["children"] as? [[String: Any]] ?? item["items"] as? [[String: Any]] ?? []
                if !nested.isEmpty {
                    group.append(UIMenu(title: label, image: image, options: item["inline"] as? Bool == true ? .displayInline : [], children: build(nested, enabled: available)))
                    continue
                }
                guard let id = item["id"] as? String else { continue }
                var attributes: UIMenuElement.Attributes = available ? [] : [.disabled]
                if item["destructive"] as? Bool == true || item["role"] as? String == "destructive" { attributes.insert(.destructive) }
                let invoke = { [weak self] in
                    guard let self, self.revision == token, self.isEnabled, self.window != nil, available else { return }
                    select(id)
                }
                let action = UIAction(title: label, image: image, identifier: UIAction.Identifier(rawValue: id),
                    attributes: attributes, state: item["checked"] as? Bool == true ? .on : .off) { _ in invoke() }
                action.accessibilityLabel = item["accessibility_label"] as? String ?? label
                group.append(action)
                let shortcut = JadixMenuShortcut(item)
                if available && !shortcut.key.isEmpty {
                    let key = token + ":" + id
                    selections[key] = invoke
                    commands.append(UIKeyCommand(title: label, image: image, action: #selector(activateShortcut(_:)),
                        input: shortcut.uiInput, modifierFlags: shortcut.uiModifiers, propertyList: key))
                }
            }
            flush(); return groups
        }
        showsMenuAsPrimaryAction = true
        menu = UIMenu(children: build(items, enabled: isEnabled))
    }
    override var keyCommands: [UIKeyCommand]? { isEnabled ? commands : [] }
    @objc private func activateShortcut(_ command: UIKeyCommand) {
        if let key = command.propertyList as? String { selections[key]?() }
    }
    override func didMoveToWindow() { super.didMoveToWindow(); if window == nil { contextMenuInteraction?.dismissMenu() } }
    func retire() {
        revision = UUID().uuidString; selections.removeAll(); commands.removeAll()
        contextMenuInteraction?.dismissMenu(); menu = nil
    }
}

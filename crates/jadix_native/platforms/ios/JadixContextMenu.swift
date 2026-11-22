import UIKit

/// Programmatic context menus use an anchored popover or an adaptive sheet.
/// Presentation owns its keyboard scope and returns once, including outside dismissal.
@MainActor final class JadixContextMenuPresentation: NSObject, UIAdaptivePresentationControllerDelegate, UIPopoverPresentationControllerDelegate {
    private weak var anchor: UIView?
    private weak var presenter: UIViewController?
    private let items: [[String: Any]]
    private var complete: (([String: Any]) -> Void)?
    private var navigation: UINavigationController?
    init(anchor: UIView, presenter: UIViewController, items: [[String: Any]], complete: @escaping ([String: Any]) -> Void) {
        self.anchor = anchor; self.presenter = presenter; self.items = items; self.complete = complete
    }
    func show() {
        guard !items.isEmpty, items.count <= 256, let anchor, let presenter, anchor.window != nil,
              presenter.presentedViewController == nil else { finish(["Err": "Menu anchor is unavailable"]); return }
        let rows = JadixMenuRows(items: items) { [weak self] id in self?.select(id) }
        let navigation = UINavigationController(rootViewController: rows); self.navigation = navigation
        navigation.modalPresentationStyle = .popover
        navigation.preferredContentSize = CGSize(width: 320, height: min(480, CGFloat(items.count) * 48 + 56))
        navigation.popoverPresentationController?.sourceView = anchor
        navigation.popoverPresentationController?.sourceRect = anchor.bounds
        navigation.popoverPresentationController?.delegate = self
        presenter.present(navigation, animated: true) { [weak self, weak navigation] in navigation?.presentationController?.delegate = self }
    }
    private func finish(_ result: [String: Any]) { let callback = complete; complete = nil; callback?(result) }
    private func select(_ id: String?) {
        navigation?.dismiss(animated: true) { [weak self] in self?.finish(["Ok": id.map { $0 as Any } ?? NSNull()]) }
    }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) { finish(["Ok": NSNull()]) }
    func popoverPresentationControllerDidDismissPopover(_ popoverPresentationController: UIPopoverPresentationController) { finish(["Ok": NSNull()]) }
    func dismiss() { complete = nil; navigation?.dismiss(animated: false); navigation = nil }
}

@MainActor private final class JadixMenuRows: UITableViewController {
    private let groups: [[[String: Any]]]
    private let select: (String?) -> Void
    init(items: [[String: Any]], select: @escaping (String?) -> Void) {
        var groups: [[[String: Any]]] = [], group: [[String: Any]] = []
        for item in items {
            if item["kind"] as? String == "separator" { if !group.isEmpty { groups.append(group); group = [] } }
            else { group.append(item) }
        }
        if !group.isEmpty { groups.append(group) }; self.groups = groups; self.select = select
        super.init(style: .insetGrouped)
        navigationItem.rightBarButtonItem = UIBarButtonItem(barButtonSystemItem: .cancel, target: self, action: #selector(cancel))
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
    override var canBecomeFirstResponder: Bool { true }
    override func viewDidAppear(_ animated: Bool) { super.viewDidAppear(animated); becomeFirstResponder() }
    @objc private func cancel() { select(nil) }
    override func accessibilityPerformEscape() -> Bool { cancel(); return true }
    override func numberOfSections(in tableView: UITableView) -> Int { groups.count }
    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { groups[section].count }
    private func children(_ item: [String: Any]) -> [[String: Any]] { item["children"] as? [[String: Any]] ?? item["items"] as? [[String: Any]] ?? [] }
    override func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let item = groups[indexPath.section][indexPath.row]
        let cell = UITableViewCell(style: .value1, reuseIdentifier: nil)
        let enabled = item["enabled"] as? Bool != false, checked = item["checked"] as? Bool == true
        cell.textLabel?.text = item["label"] as? String
        cell.textLabel?.textColor = item["destructive"] as? Bool == true || item["role"] as? String == "destructive" ? .systemRed : .label
        let source = item["symbol"] as? String ?? (item["icon"] as? [String: Any])?["source"] as? String
        cell.imageView?.image = source.flatMap { UIImage(systemName: $0.replacingOccurrences(of: "sf:", with: "")) }
        cell.detailTextLabel?.text = item["accelerator"] as? String ?? item["key"] as? String
        cell.accessoryType = !children(item).isEmpty ? .disclosureIndicator : checked ? .checkmark : .none
        cell.isUserInteractionEnabled = enabled; cell.contentView.alpha = enabled ? 1 : 0.45
        cell.accessibilityLabel = item["accessibility_label"] as? String ?? item["label"] as? String
        cell.accessibilityTraits = enabled ? [.button] : [.button, .notEnabled]
        if checked { cell.accessibilityTraits.insert(.selected) }
        return cell
    }
    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) { activate(groups[indexPath.section][indexPath.row]) }
    private func activate(_ item: [String: Any]) {
        guard item["enabled"] as? Bool != false else { return }
        let nested = children(item)
        if !nested.isEmpty { let next = JadixMenuRows(items: nested, select: select); next.title = item["label"] as? String; navigationController?.pushViewController(next, animated: true) }
        else { select(item["id"] as? String) }
    }
    override var keyCommands: [UIKeyCommand]? {
        var commands = [UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(cancel))]
        for (index, item) in groups.flatMap({ $0 }).enumerated() where item["enabled"] as? Bool != false {
            let shortcut = JadixMenuShortcut(item)
            guard !shortcut.key.isEmpty else { continue }
            let command = UIKeyCommand(title: item["label"] as? String ?? "", image: nil, action: #selector(JadixMenuRows.shortcut(_:)), input: shortcut.uiInput, modifierFlags: shortcut.uiModifiers, propertyList: index)
            commands.append(command)
        }
        return commands
    }
    @objc private func shortcut(_ command: UIKeyCommand) {
        let items = groups.flatMap { $0 }
        if let index = command.propertyList as? Int, items.indices.contains(index) { activate(items[index]) }
    }
}

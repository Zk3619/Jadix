import UIKit

@MainActor @discardableResult func jadixFocusView(_ view: UIView) -> Bool {
    guard !view.isHidden, view.alpha > 0.01, view.isUserInteractionEnabled,
          (view as? UIControl)?.isEnabled != false else { return false }
    // Composite leaves keep their native editor/control as the responder.
    if view is UITextField || view is UITextView || view is UIControl {
        if view.becomeFirstResponder() { return true }
    }
    if let container = view as? JadixContainer, container.canBecomeFirstResponder {
        return container.becomeFirstResponder()
    }
    for child in view.subviews { if jadixFocusView(child) { return true } }
    return view is JadixContainer && view.becomeFirstResponder()
}

/// The shared wire owns logical dimensions and spans; UIKit exposes them without
/// allocating a dense grid or retaining views removed by virtualization.
@MainActor final class JadixGridCell: UIAccessibilityElement, UIAccessibilityContainerDataTableCell {
    weak var target: UIView?
    weak var grid: JadixContainer?
    var row = NSRange(location: 0, length: 1), column = NSRange(location: 0, length: 1)
    var emptyFrame = CGRect.zero
    func accessibilityRowRange() -> NSRange { row }
    func accessibilityColumnRange() -> NSRange { column }
    override var accessibilityFrame: CGRect {
        get {
            if let target { return UIAccessibility.convertToScreenCoordinates(target.bounds, in: target) }
            guard let grid else { return emptyFrame }
            let rect = CGRect(x: CGFloat(column.location) * grid.bounds.width / CGFloat(max(1, grid.columns)),
                              y: CGFloat(row.location) * grid.bounds.height / CGFloat(max(1, grid.rows)),
                              width: grid.bounds.width / CGFloat(max(1, grid.columns)), height: grid.bounds.height / CGFloat(max(1, grid.rows)))
            return UIAccessibility.convertToScreenCoordinates(rect, in: grid)
        }
        set { emptyFrame = newValue }
    }
    override func accessibilityActivate() -> Bool {
        guard let target else { return false }
        return target.accessibilityActivate() || jadixFocusView(target)
    }
    override func accessibilityIncrement() { target?.accessibilityIncrement() }
    override func accessibilityDecrement() { target?.accessibilityDecrement() }
}

@MainActor final class JadixContainer: UIView, UIAccessibilityContainerDataTable {
    let keyboardInput = JadixKeyboardInput()
    var rows = 0, columns = 0
    private var cells: [String: JadixGridCell] = [:], empty: [String: JadixGridCell] = [:]
    private var headers: [JadixGridCell] = [], orderedCells: [JadixGridCell] = []
    override var canBecomeFirstResponder: Bool { keyboardInput.focusable }
    override func becomeFirstResponder() -> Bool {
        let previous = isFirstResponder, result = super.becomeFirstResponder()
        if result && !previous { keyboardInput.focus(true) }; return result
    }
    override func resignFirstResponder() -> Bool {
        let previous = isFirstResponder, result = super.resignFirstResponder()
        if result && previous { keyboardInput.focus(false) }; return result
    }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        if touches.contains(where: { $0.view === self }) { _ = becomeFirstResponder() }
        super.touchesBegan(touches, with: event)
    }
    override var keyCommands: [UIKeyCommand]? { keyboardInput.commands(action: #selector(keyCommand(_:))) }
    @objc private func keyCommand(_ command: UIKeyCommand) { keyboardInput.command(command) }
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let remaining = keyboardInput.presses(presses, down: true)
        if !remaining.isEmpty { super.pressesBegan(remaining, with: event) }
    }
    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let remaining = keyboardInput.presses(presses, down: false)
        if !remaining.isEmpty { super.pressesEnded(remaining, with: event) }
    }
    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        keyboardInput.cancel(); super.pressesCancelled(presses, with: event)
    }
    func accessibilityRowCount() -> Int { rows }
    func accessibilityColumnCount() -> Int { columns }
    func accessibilityDataTableCellElement(forRow row: Int, column: Int) -> (any UIAccessibilityContainerDataTableCell)? {
        guard (0..<rows).contains(row), (0..<columns).contains(column) else { return nil }
        if let cell = orderedCells.first(where: { NSLocationInRange(row, $0.row) && NSLocationInRange(column, $0.column) }) { return cell }
        let key = "\(row):\(column)"
        if let cell = empty[key] { return cell }
        let cell = JadixGridCell(accessibilityContainer: self)
        cell.row = NSRange(location: row, length: 1); cell.column = NSRange(location: column, length: 1)
        cell.accessibilityLabel = ""; cell.accessibilityTraits = .staticText
        cell.grid = self
        if empty.count >= 256 { empty.removeAll() }
        empty[key] = cell; return cell
    }
    func accessibilityHeaderElements(forColumn column: Int) -> [any UIAccessibilityContainerDataTableCell]? {
        headers.filter { NSLocationInRange(column, $0.column) }
    }
    func configureGrid(_ grid: [String: Any]?, entries: [(String, UIView, [String: Any])]) {
        rows = grid?["rows"] as? Int ?? 0; columns = grid?["columns"] as? Int ?? 0
        accessibilityContainerType = grid == nil ? .none : .dataTable
        guard grid != nil else { cells.removeAll(); headers.removeAll(); orderedCells.removeAll(); empty.removeAll(); accessibilityElements = nil; return }
        var next: [String: JadixGridCell] = [:], ordered: [JadixGridCell] = [], nextHeaders: [JadixGridCell] = []
        for (id, view, descriptor) in entries {
            guard let span = descriptor["cell"] as? [String: Int] ?? descriptor["columnHeader"] as? [String: Int] else { continue }
            let cell = cells[id] ?? JadixGridCell(accessibilityContainer: self)
            cell.target = view; cell.row = NSRange(location: span["row"] ?? 0, length: span["rowSpan"] ?? 1)
            cell.column = NSRange(location: span["column"] ?? 0, length: span["columnSpan"] ?? 1)
            cell.accessibilityLabel = view.accessibilityLabel ?? descriptor["text"] as? String ?? ""
            cell.accessibilityValue = view.accessibilityValue; cell.accessibilityHint = view.accessibilityHint
            cell.accessibilityTraits = view.accessibilityTraits
            if descriptor["columnHeader"] is [String: Any] { cell.accessibilityTraits.insert(.header); nextHeaders.append(cell) }
            next[id] = cell; ordered.append(cell)
        }
        cells = next; headers = nextHeaders; orderedCells = ordered; empty.removeAll()
        accessibilityElements = ordered
    }
}

@MainActor class JadixButton: UIButton {
    let keyboardInput = JadixKeyboardInput()
    override var canBecomeFirstResponder: Bool { true }
    override func becomeFirstResponder() -> Bool {
        let previous = isFirstResponder, result = super.becomeFirstResponder()
        if result && !previous { keyboardInput.focus(true) }; return result
    }
    override func resignFirstResponder() -> Bool {
        let previous = isFirstResponder, result = super.resignFirstResponder()
        if result && previous { keyboardInput.focus(false) }; return result
    }
    override var keyCommands: [UIKeyCommand]? { keyboardInput.commands(action: #selector(keyCommand(_:))) }
    @objc private func keyCommand(_ command: UIKeyCommand) { keyboardInput.command(command) }
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let remaining = keyboardInput.presses(presses, down: true)
        if !remaining.isEmpty { super.pressesBegan(remaining, with: event) }
    }
    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let remaining = keyboardInput.presses(presses, down: false)
        if !remaining.isEmpty { super.pressesEnded(remaining, with: event) }
    }
    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        keyboardInput.cancel(); super.pressesCancelled(presses, with: event)
    }
    override func accessibilityElementDidBecomeFocused() { super.accessibilityElementDidBecomeFocused(); _ = becomeFirstResponder() }
    override func didUpdateFocus(in context: UIFocusUpdateContext, with coordinator: UIFocusAnimationCoordinator) {
        super.didUpdateFocus(in: context, with: coordinator)
        if context.nextFocusedView === self { _ = becomeFirstResponder() }
    }
}

/// Only generic focus targets use this bridge. Native editors and hosted views
/// keep UIKit's text/IME pipeline. Held keys never outlive responder ownership.
@MainActor final class JadixKeyboardInput {
    weak var view: UIView?
    var emit: (([String: Any]) -> Void)?
    var focusable = false
    var activationKeys: Set<Int> = []
    var policy: [String: Any] = [:]
    private var held: Set<Int> = []
    private var consumed: Set<Int> = []
    private var activating: Set<Int> = []
    private static let special: [Int: String] = [40: "Enter", 41: "Escape", 42: "Backspace", 43: "Tab", 44: " ",
        57: "CapsLock", 70: "PrintScreen", 71: "ScrollLock", 72: "Pause", 73: "Insert", 74: "Home", 75: "PageUp",
        76: "Delete", 77: "End", 78: "PageDown", 79: "ArrowRight", 80: "ArrowLeft", 81: "ArrowDown", 82: "ArrowUp",
        83: "NumLock", 88: "Enter", 101: "ContextMenu", 224: "Control", 225: "Shift", 226: "Alt", 227: "Meta",
        228: "Control", 229: "Shift", 230: "Alt", 231: "Meta"]
    private var active: Bool {
        guard let view, view.isFirstResponder, !view.isHidden, view.isUserInteractionEnabled,
              (view as? UIControl)?.isEnabled != false, let window = view.window else { return false }
        return window.isKeyWindow && window.windowScene?.activationState == .foregroundActive
    }
    func configure(_ policy: [String: Any], view: UIView, emit: @escaping ([String: Any]) -> Void) {
        self.view = view; self.emit = emit; self.policy = policy
        focusable = policy["focusable"] as? Bool == true
        if view is JadixContainer && !focusable && view.isFirstResponder { _ = view.resignFirstResponder() }
    }
    func reset() { cancel(); emit = nil; view = nil; policy = [:]; focusable = false; activationKeys.removeAll() }
    func cancel() { held.removeAll(); consumed.removeAll(); activating.removeAll() }
    func focus(_ focused: Bool) { if !focused { cancel() }; emit?(["kind": focused ? "focus" : "blur"]) }
    private func modifiers(_ flags: UIKeyModifierFlags) -> [String: Bool] {
        ["shift": flags.contains(.shift), "control": flags.contains(.control), "alt": flags.contains(.alternate),
         "meta": flags.contains(.command), "capsLock": flags.contains(.alphaShift)]
    }
    private func flags(_ modifiers: [String: Bool]) -> UIKeyModifierFlags {
        var result: UIKeyModifierFlags = []
        let options: [(String, UIKeyModifierFlags)] = [("shift", .shift), ("control", .control), ("alt", .alternate), ("meta", .command), ("capsLock", .alphaShift)]
        for (name, flag) in options {
            if modifiers[name] == true { result.insert(flag) }
        }
        return result
    }
    private func name(_ key: UIKey) -> String {
        let code = key.keyCode.rawValue
        if let special = Self.special[code] { return special }
        if (58...69).contains(code) { return "F\(code - 57)" }
        if (104...115).contains(code) { return "F\(code - 91)" }
        if !key.modifierFlags.intersection([.control, .command]).isEmpty { return key.charactersIgnoringModifiers }
        return key.characters.isEmpty ? key.charactersIgnoringModifiers : key.characters
    }
    private func input(_ name: String) -> String? {
        let special = ["Enter": "\r", "Escape": UIKeyCommand.inputEscape, "Backspace": "\u{8}", "Tab": "\t",
            "Delete": "\u{7f}", "Home": UIKeyCommand.inputHome, "End": UIKeyCommand.inputEnd,
            "PageUp": UIKeyCommand.inputPageUp, "PageDown": UIKeyCommand.inputPageDown,
            "ArrowLeft": UIKeyCommand.inputLeftArrow, "ArrowRight": UIKeyCommand.inputRightArrow,
            "ArrowUp": UIKeyCommand.inputUpArrow, "ArrowDown": UIKeyCommand.inputDownArrow]
        if let value = special[name] { return value }
        if name.hasPrefix("F"), let number = Int(name.dropFirst()), (1...24).contains(number), let scalar = UnicodeScalar(0xF703 + number) { return String(scalar) }
        return name.count == 1 ? name : nil
    }
    func commands(action: Selector) -> [UIKeyCommand] {
        guard active else { return [] }
        return (policy["shortcuts"] as? [[String: Any]] ?? []).prefix(64).compactMap { shortcut in
            guard let name = shortcut["key"] as? String, let input = input(name) else { return nil }
            let command = UIKeyCommand(title: shortcut["id"] as? String ?? "", image: nil, action: action,
                input: input, modifierFlags: flags(shortcut["modifiers"] as? [String: Bool] ?? [:]), propertyList: shortcut["id"])
            command.wantsPriorityOverSystemBehavior = true
            return command
        }
    }
    func command(_ command: UIKeyCommand) {
        guard active, let id = command.propertyList as? String,
              (policy["shortcuts"] as? [[String: Any]] ?? []).contains(where: { $0["id"] as? String == id }), consumed.isEmpty else { return }
        emit?(["kind": "shortcut", "id": id])
    }
    func presses(_ presses: Set<UIPress>, down: Bool) -> Set<UIPress> {
        guard active else { cancel(); return presses }
        var remaining = presses
        for press in presses {
            guard let key = press.key, (4...231).contains(key.keyCode.rawValue) else { continue }
            let code = key.keyCode.rawValue, name = name(key), mods = modifiers(key.modifierFlags)
            let transition = down ? held.insert(code).inserted : held.remove(code) != nil
            if consumed.contains(code) { remaining.remove(press) }
            guard transition else { continue }
            if !down { consumed.remove(code) }
            if policy[down ? "keyDown" : "keyUp"] as? Bool == true, !name.isEmpty, name.utf8.count <= 128 {
                emit?(["kind": "key", "event": ["phase": down ? "down" : "up", "key": name, "code": code, "modifiers": mods, "repeat": false]])
            }
            guard active else { cancel(); return remaining }
            if !down && activating.remove(code) != nil {
                remaining.remove(press); emit?(["kind": "activate"]); continue
            }
            // UIKeyCommand handles normal combinations. This also covers keys
            // which UIKit cannot represent as a key-command input.
            if down, let shortcut = (policy["shortcuts"] as? [[String: Any]] ?? []).first(where: {
                $0["key"] as? String == name && flags($0["modifiers"] as? [String: Bool] ?? [:]) == key.modifierFlags.intersection([.shift, .control, .alternate, .command, .alphaShift])
            }), let id = shortcut["id"] as? String {
                consumed.insert(code); remaining.remove(press); emit?(["kind": "shortcut", "id": id])
            } else if down && activationKeys.contains(code) && key.modifierFlags.intersection([.shift, .control, .alternate, .command]).isEmpty {
                activating.insert(code); consumed.insert(code); remaining.remove(press)
            }
        }
        return remaining
    }
}

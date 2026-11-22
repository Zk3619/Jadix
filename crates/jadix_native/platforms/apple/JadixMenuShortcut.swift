import Foundation

/// The Rust KeyChord parser validates authored accelerators. Apple adapters only
/// translate the retained bound symbol and modifier names into platform keys.
struct JadixMenuShortcut {
    let key: String
    let command: Bool, control: Bool, alt: Bool, shift: Bool
    init(_ item: [String: Any]) {
        var key = item["key"] as? String ?? ""
        var names = item["modifiers"] as? [String] ?? []
        if let accelerator = item["accelerator"] as? String {
            let parts = accelerator.split(separator: "+", omittingEmptySubsequences: false).map(String.init)
            key = parts.last ?? ""; names = Array(parts.dropLast())
        }
        let modifiers = Set(names.map { $0.lowercased() })
        command = !modifiers.isDisjoint(with: ["cmd", "command", "super", "meta", "cmdorctrl"])
        control = !modifiers.isDisjoint(with: ["ctrl", "control"])
        alt = !modifiers.isDisjoint(with: ["alt", "option"])
        shift = modifiers.contains("shift")
        if key.lowercased().hasPrefix("f"), let number = Int(key.dropFirst()), (1...24).contains(number),
           let scalar = UnicodeScalar(0xf703 + number) { key = String(scalar) }
        self.key = key.lowercased()
    }
}

#if os(iOS)
import UIKit

extension JadixMenuShortcut {
    var uiInput: String {
        switch key {
        case "arrowup", "up": return UIKeyCommand.inputUpArrow
        case "arrowdown", "down": return UIKeyCommand.inputDownArrow
        case "arrowleft", "left": return UIKeyCommand.inputLeftArrow
        case "arrowright", "right": return UIKeyCommand.inputRightArrow
        case "escape", "esc": return UIKeyCommand.inputEscape
        case "home": return UIKeyCommand.inputHome
        case "end": return UIKeyCommand.inputEnd
        case "pageup": return UIKeyCommand.inputPageUp
        case "pagedown": return UIKeyCommand.inputPageDown
        case "enter", "return": return "\r"
        case "tab": return "\t"
        case "backspace", "delete": return "\u{8}"
        case "space": return " "
        default: return key
        }
    }
    var uiModifiers: UIKeyModifierFlags {
        var flags: UIKeyModifierFlags = []
        if command { flags.insert(.command) }; if control { flags.insert(.control) }
        if alt { flags.insert(.alternate) }; if shift { flags.insert(.shift) }
        return flags
    }
}
#endif

import Foundation

/// Both Apple adapters use xterm's modifier parameter and application-cursor mode.
enum JadixTerminalInput {
    static func special(_ key: String, shift: Bool, alt: Bool, control: Bool, applicationCursor: Bool) -> String? {
        let modifier = 1 + (shift ? 1 : 0) + (alt ? 2 : 0) + (control ? 4 : 0)
        if let final = ["up": "A", "down": "B", "right": "C", "left": "D", "home": "H", "end": "F"][key] {
            return modifier == 1 ? "\u{1b}" + (applicationCursor ? "O" : "[") + final : "\u{1b}[1;\(modifier)\(final)"
        }
        if let number = ["insert": 2, "delete": 3, "page_up": 5, "page_down": 6,
                         "f5": 15, "f6": 17, "f7": 18, "f8": 19, "f9": 20, "f10": 21, "f11": 23, "f12": 24][key] {
            return "\u{1b}[\(number)" + (modifier == 1 ? "~" : ";\(modifier)~")
        }
        if let final = ["f1": "P", "f2": "Q", "f3": "R", "f4": "S"][key] {
            return modifier == 1 ? "\u{1b}O\(final)" : "\u{1b}[1;\(modifier)\(final)"
        }
        switch key {
        case "tab": return shift ? "\u{1b}[Z" : (alt ? "\u{1b}\t" : "\t")
        case "return": return alt ? "\u{1b}\r" : "\r"
        case "backspace": return alt ? "\u{1b}\u{7f}" : "\u{7f}"
        case "escape": return "\u{1b}"
        default: return nil
        }
    }
}

import AppKit

@MainActor
func jadixEnvironmentSnapshot(_ view: NSView) -> [String: Any] {
    let preferences = jadixLocalePreferences()
    let languages = preferences["languages"] as? [String] ?? ["en-US"]
    let dark = view.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    return ["platform": ["name": "macos", "os": "macos"], "locale": preferences,
            "color_scheme": dark ? "dark" : "light",
            "layout_direction": Locale.characterDirection(forLanguage: languages[0]) == .rightToLeft ? "right-to-left" : "left-to-right",
            "high_contrast": NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast,
            "reduced_motion": NSWorkspace.shared.accessibilityDisplayShouldReduceMotion,
            "font_scale": 1.0,
            "lifecycle": NSApp.isHidden ? "background" : (NSApp.isActive ? "active" : "inactive")]
}
